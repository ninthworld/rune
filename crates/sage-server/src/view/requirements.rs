//! Building target `requirement` slots and their candidate labels.

use super::*;

/// The stable requirement-slot id for the blockers assigned to `attacker` in a
/// [`Action::DeclareBlockers`] projection. One slot per declared attacker, keyed by
/// the attacker's permanent id, so the returned choice names which attacker the
/// selected blockers are assigned to. Recomputed (never parsed) on resolution.
pub(crate) fn blocker_slot(attacker: PermanentId) -> String {
    format!("block_{}", attacker.0)
}

/// The combat-damage assignment-order slot for a multi-blocked attacker (CR 510.1,
/// issue #346), keyed by the attacker's permanent id so the returned permutation
/// names which attacker it orders. Recomputed (never parsed) on resolution.
pub(crate) fn damage_order_slot(attacker: PermanentId) -> String {
    format!("order_{}", attacker.0)
}

/// One `order` prompt per attacker that owes a combat-damage assignment order
/// ([`attackers_needing_damage_order`], issue #346): the prompt's `items` are that
/// attacker's blockers in battlefield order, and the client returns a permutation of
/// them. Empty when no attacker is multi-blocked (a choice-free action).
pub(crate) fn damage_order_prompts(state: &GameState, db: &CardDatabase) -> Vec<Prompt> {
    attackers_needing_damage_order(state)
        .into_iter()
        .map(|attacker| {
            let items: Vec<String> = state
                .battlefield
                .iter()
                .filter(|p| p.blocking.contains(&attacker))
                .map(|p| permanent_entity_id(p.id))
                .collect();
            Prompt::Order {
                slot: damage_order_slot(attacker),
                prompt: format!(
                    "Order damage assignment for {}",
                    permanent_card_name(state, attacker, db)
                ),
                items,
            }
        })
        .collect()
}

/// The per-attacker defender-choice slot of a multiplayer [`Action::DeclareAttackers`]
/// (CR 508.1a, issue #345), keyed by the attacker's permanent id so the returned
/// choice names which attacker the selected defender is assigned to — the exact
/// parallel of [`blocker_slot`]. Recomputed (never parsed) on resolution.
pub(crate) fn defender_slot(attacker: PermanentId) -> String {
    format!("defend_{}", attacker.0)
}

/// The slot id of the mulligan bottoming pick (CR 103.5, London).
pub(crate) const BOTTOM_SLOT: &str = "bottom";

/// The bottoming slot for a mulligan [`Action::Keep`] (CR 103.5, London): the
/// [`bottom_requirement`] candidates (the deciding seat's hand cards) projected as a
/// single [`Prompt::SelectFromZone`] over that hand carrying the **exact owed
/// count** — the shape `docs/protocol.md` and the cross-language contract fixture
/// have always documented for bottoming. It rode a `requirements` slot until issue
/// #451: that shape carries no count, so a client could only recover "how many" by
/// parsing the prompt text and could not stop a wrong-sized keep before sending it.
/// Empty for a first-hand keep (nothing owed), so that keep stays a plain,
/// choice-free action.
pub(crate) fn keep_prompts(state: &GameState, action: &Action) -> Vec<Prompt> {
    match bottom_requirement(state, action) {
        Some(req) => vec![Prompt::SelectFromZone {
            slot: BOTTOM_SLOT.to_string(),
            prompt: format!("Put {} card(s) on the bottom of your library", req.count),
            zone: "hand".to_string(),
            owner: player_id(state.priority),
            count: count(req.count),
            // A bottoming is exact: put down precisely as many as you mulliganed.
            min: None,
            candidates: req.candidates.into_iter().map(target_entity_id).collect(),
        }],
        None => Vec::new(),
    }
}

/// The attacker-declaration requirement slots (CR 508.1a): the engine's
/// [`attacker_candidates`] as one multi-select `attackers` slot, plus — only when there
/// is more than one thing to attack (issue #341/#345, widened by #608) — one
/// `defend_<id>` slot per attacker candidate listing the attack-target candidates
/// ([`defender_candidates`], as player *or planeswalker* entity ids) that attacker may
/// be assigned to.
///
/// Empty when no creature may attack, so declaring no attackers stays a plain,
/// choice-free action. In a two-player game **with no planeswalker to attack** the sole
/// opponent is the only target, so no `defend_*` slot is offered and the wire is exactly
/// as before — the client gains no extra step (issue #347) and [`bind_attackers`]
/// assigns that sole target. The moment an opponent has a planeswalker there really is
/// a choice, even at two seats, and the slot appears: the gate is the number of
/// *targets*, not the number of opponents.
pub(crate) fn attacker_requirements(
    state: &GameState,
    db: &CardDatabase,
) -> Vec<TargetRequirement> {
    let candidates = attacker_candidates(state, db);
    if candidates.is_empty() {
        return Vec::new();
    }
    let mut reqs = vec![TargetRequirement {
        slot: "attackers".to_string(),
        prompt: "Choose which creatures attack".to_string(),
        // Declaring **no** attackers is a legal declaration (CR 508.1a), and this is the
        // field that says a slot may be left unanswered — so it says so. The resolve path
        // has always treated it that way (an empty declaration binds directly, without
        // passing through `targets_fill_requirements`); stating it is what lets a client
        // tell "you may choose none of these" from "this must be filled or the submission
        // is rejected", which is otherwise the same shape on the wire.
        optional: true,
        candidates: candidates
            .iter()
            .copied()
            .map(permanent_entity_id)
            .collect(),
        // CR 508.1f / CR 702.20b: which of these creatures tapping is part of attacking
        // with. The engine's predicate, so a client draws a declaration it is still
        // assembling without judging a keyword ([`attacking_taps`]).
        taps: candidates
            .iter()
            .copied()
            .filter(|&attacker| attacking_taps(state, attacker, db))
            .map(permanent_entity_id)
            .collect(),
        // The declaration as a whole, not any one attacker's choice.
        subject: None,
    }];
    // Each attacker chooses what it attacks. With a single candidate there is nothing
    // to choose, so no defender slots are offered.
    let defenders = defender_candidates(state, db);
    if defenders.len() > 1 {
        let defender_ids: Vec<String> = defenders
            .iter()
            .copied()
            .map(attack_target_entity_id)
            .collect();
        for attacker in candidates {
            reqs.push(TargetRequirement {
                slot: defender_slot(attacker),
                optional: false,
                prompt: format!(
                    "Choose what {} attacks",
                    permanent_card_name(state, attacker, db)
                ),
                candidates: defender_ids.clone(),
                // Choosing what an attacker attacks taps nothing; the attacker's own tap
                // rides on the slot that declared it.
                taps: Vec::new(),
                // Whose choice this is, stated rather than encoded in the slot id: a
                // client asks one attacker at a time and draws the arrow from it.
                subject: Some(permanent_entity_id(attacker)),
            });
        }
    }
    reqs
}

/// The blocker-declaration requirement slots (CR 509.1a) for the player who owes
/// the current declaration ([`pending_blocker_declarer`]): one slot per attacker
/// *attacking that player*, each listing the eligible blockers they control
/// ([`blocker_candidates_for`]) **that may legally block that attacker**
/// ([`blocker_can_block_attacker`]). Empty when there is nothing for this declarer to
/// block or no creature to block with, so declaring no blockers stays a plain,
/// choice-free action. In a two-player game the sole opponent is the declarer and
/// every attacker attacks them, so this is unchanged; with attackers split across
/// several defenders (issue #344) each declarer sees only their own sub-combat.
///
/// The per-attacker filter is what makes the pairwise evasion rules visible instead of
/// merely enforced: an attacker that can't be blocked at all gets **no slot**, and one
/// that can't be blocked by black creatures lists only the rest. The engine still
/// decides — the server asks [`blocker_can_block_attacker`] per pair and does no rules
/// reasoning of its own — and the declaration is re-validated on submit, so this is a
/// projection of legality rather than a second copy of it.
pub(crate) fn blocker_requirements(state: &GameState, db: &CardDatabase) -> Vec<TargetRequirement> {
    let Some(declarer) = pending_blocker_declarer(state) else {
        return Vec::new();
    };
    let attackers: Vec<_> = declared_attackers(state)
        .into_iter()
        .filter(|&attacker| attacking_defender_of(state, attacker) == Some(declarer))
        .collect();
    let blockers = blocker_candidates_for(state, declarer, db);
    if attackers.is_empty() || blockers.is_empty() {
        return Vec::new();
    }
    attackers
        .into_iter()
        .filter_map(|attacker| {
            let candidates: Vec<String> = blockers
                .iter()
                .copied()
                .filter(|&blocker| blocker_can_block_attacker(state, attacker, blocker, db))
                .map(permanent_entity_id)
                .collect();
            // An attacker nothing may block is not a choice; offering an empty slot
            // would ask the player a question with no answer.
            if candidates.is_empty() {
                return None;
            }
            Some(TargetRequirement {
                slot: blocker_slot(attacker),
                // Blocking with nothing is a legal declaration (CR 509.1a), and this slot
                // is how a declarer says so — the same statement the attackers slot makes,
                // for the same reason: the resolve path binds an empty declaration
                // directly, and a client can only know that if the wire says it.
                optional: true,
                prompt: blocker_prompt(state, attacker, db),
                candidates,
                // Blocking does not tap (CR 509.1): a blocker assigned here is drawn
                // exactly as it stands.
                taps: Vec::new(),
                // Which attacker this slot assigns blockers to — the same statement
                // the defender slots carry, from the other side of the combat.
                subject: Some(permanent_entity_id(attacker)),
            })
        })
        .collect()
}

/// The prompt for one attacker's blocker slot, naming any restriction on *how many*
/// blockers the declaration may assign to it.
///
/// The block-count restrictions — menace's floor of two (CR 702.110b) and the "no more
/// than one creature" ceiling (CR 509.1b) — are constraints on the whole selection
/// rather than on any one blocker, so the engine can only reject them once the
/// declaration is assembled, which would otherwise reach the player as a submit that
/// silently does nothing. Saying so in the prompt is the fix that keeps the rule where
/// it belongs: the *server* asks the rules authority
/// ([`sage_engine::permanent_has_menace`], [`sage_engine::blocked_by_at_most_one`]) and
/// puts the answer in words, and the client still computes no legality of its own
/// (`AGENTS.md`, zero game logic in the client).
///
/// The pairwise restrictions need no words here: they are already visible as the slot's
/// candidate list.
fn blocker_prompt(state: &GameState, attacker: PermanentId, db: &CardDatabase) -> String {
    let name = permanent_card_name(state, attacker, db);
    // Both bounds at once is possible in principle and unsatisfiable in practice, so it
    // is stated as what it is rather than as two rules a player has to combine.
    let note = match (
        sage_engine::permanent_has_menace(state, attacker, db),
        sage_engine::blocked_by_at_most_one(state, attacker, db),
    ) {
        (true, true) => Some("no legal block — two or more, but no more than one"),
        // "or none": a count restriction governs how a creature is blocked, never
        // whether it is.
        (true, false) => Some("menace — two or more, or none"),
        (false, true) => Some("no more than one blocker"),
        (false, false) => None,
    };
    match note {
        Some(note) => format!("Choose blockers for {name} ({note})"),
        None => format!("Choose blockers for {name}"),
    }
}

/// The ability-target requirement slots (ADR 0004 §Enumeration, deferral #73): the
/// engine's per-slot [`target_requirements`] candidate sets projected one slot each
/// (`t0`, `t1`, …), reusing the same content-binding machinery as the mulligan and
/// combat multi-selects. Empty for a non-targeting ability.
pub(crate) fn ability_requirements(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Vec<TargetRequirement> {
    target_requirements(state, db, action)
        .into_iter()
        .enumerate()
        .map(|(index, req)| TargetRequirement {
            slot: format!("t{index}"),
            prompt: target_spec_prompt(req.spec).to_string(),
            // The slots past a group's minimum may be left empty ("up to two target
            // creatures"). The engine decides which; the projection only carries it.
            optional: req.optional,
            candidates: req.candidates.into_iter().map(target_entity_id).collect(),
            // Choosing a target does nothing to the target. What a spell costs is posed
            // on its own `pay_mana` slots, and each of those states its own tapping.
            taps: Vec::new(),
            // A spell or ability's own target slot is about the action, not about one
            // of the objects the action names.
            subject: None,
        })
        .collect()
}

/// The dock label for one ability activation: the ability's own generated rules
/// sentence (`ability_text`, ADR 0008), resolved through the same
/// [`abilities_of`] index the engine action names — so the words a player clicks
/// are exactly the words the card prints, and two abilities on one permanent
/// never share a label. Falls back to the old generic label if the permanent or
/// index cannot be resolved (defensive: an offered action always names a live
/// ability).
pub(crate) fn ability_label(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> String {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == permanent)
        .and_then(|perm| {
            let name = permanent_name(perm, db);
            abilities_of_permanent(state, db, perm)
                .get(index)
                .map(|ability| ability_text(&name, ability))
        })
        .unwrap_or_else(|| "Activate ability".to_string())
}

/// The label for aiming a triggered ability: the ability's own sentence, drawn from
/// the effects it carries on the stack ([`effects_description`]) — the same words the
/// stack entry shows, so the prompt and the stack cannot describe it differently.
pub(crate) fn trigger_label(state: &GameState, db: &CardDatabase, ability: StackId) -> String {
    state
        .stack
        .iter()
        .find(|o| o.id == ability)
        .and_then(|o| match &o.kind {
            StackObjectKind::Ability {
                source, effects, ..
            } => Some(effects_description(
                &source
                    .permanent()
                    .and_then(|id| state.battlefield.iter().find(|p| p.id == id))
                    .map_or_else(
                        || "This ability's source".to_string(),
                        |p| permanent_name(p, db),
                    ),
                effects,
            )),
            StackObjectKind::Spell { .. } => None,
        })
        .unwrap_or_else(|| "Choose targets".to_string())
}

/// The subjects a trigger-aiming action binds to: the trigger **where it sits on the
/// stack**, and the permanent whose ability is asking.
///
/// Both, because both are places a player looks and neither alone covers the gesture.
/// The trigger is the object holding the game up and the one named in the column that
/// says so, which is the first thing anyone clicks; the source is what the ability
/// belongs to, and naming it is what highlights the card on the board. Listing the two
/// costs a client nothing — a subject list is already plural, and any id in it reaches
/// the same action.
pub(crate) fn trigger_subject(state: &GameState, ability: StackId) -> Vec<String> {
    state
        .stack
        .iter()
        .find(|o| o.id == ability)
        .and_then(|o| match &o.kind {
            StackObjectKind::Ability { source, .. } => Some(
                std::iter::once(stack_entity_id(ability))
                    .chain(source.permanent().map(permanent_entity_id))
                    .collect(),
            ),
            StackObjectKind::Spell { .. } => None,
        })
        .unwrap_or_default()
}

/// The display name of the permanent `id` on the battlefield, for a human prompt,
/// or a stable placeholder if it is not found.
fn permanent_card_name(state: &GameState, id: PermanentId, db: &CardDatabase) -> String {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .map(|perm| permanent_name(perm, db))
        .unwrap_or_else(|| "the attacker".to_string())
}

#[cfg(test)]
mod menace_prompt_tests;
#[cfg(test)]
mod planeswalker_attack_tests;
#[cfg(test)]
mod tests;
