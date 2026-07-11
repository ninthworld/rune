//! Deriving personalized [`rune_protocol`] views from engine [`GameState`].
//!
//! This is the engine→protocol shim for the room task (issue #31). The engine
//! speaks in seat indices and card ids; the wire protocol speaks in opaque string
//! entity ids and redacts hidden zones. Everything here is a pure translation of
//! an already-computed state — it holds **no game logic** (the engine owns
//! legality and effects) and does no I/O. It only decides what one seat is allowed
//! to see and how to name the entities the engine already produced.
//!
//! Kept inside `rune-server` deliberately: it maps between two crates the server
//! already depends on, and adds nothing to the wire contract in `rune-protocol`.

use rune_engine::{
    valid_actions, Action, CardData, CardDatabase, CardId, CardInstance, CardInstanceId,
    CounterKind, Effect, GameState, PermanentId, Player, PlayerId, StackId, StackObject,
    StackObjectKind, Step,
};
use rune_protocol::{
    CardView, ChooseAction, Counter, GameView, OpponentView, Permanent as PermanentView, Phase,
    StackItem, TargetChoice, TargetRequirement, ValidAction, ZonePile,
};

/// The opaque protocol id for a seat (an engine [`PlayerId`]).
fn player_id(seat: PlayerId) -> String {
    format!("p{}", seat.0)
}

/// The opaque protocol id for a card referenced from a hand or a public pile.
///
/// Keyed by the per-copy [`CardInstanceId`], so two copies of the same printing
/// get distinct entity ids (`card_5` vs `card_6`) and the action a client echoes
/// back names an unambiguous instance — the engine no longer falls back to "the
/// first matching copy".
fn card_entity_id(instance: CardInstanceId) -> String {
    format!("card_{}", instance.0)
}

/// The opaque protocol id for a permanent on the battlefield.
fn permanent_entity_id(id: PermanentId) -> String {
    format!("perm_{}", id.0)
}

/// The wire name for an engine [`CounterKind`], as the client expects it in
/// [`Counter::kind`] (e.g. `"+1/+1"`). Kept exhaustive so a new engine variant
/// forces a matching wire string here rather than silently going unnamed.
fn counter_kind_str(kind: CounterKind) -> &'static str {
    match kind {
        CounterKind::PlusOnePlusOne => "+1/+1",
        CounterKind::MinusOneMinusOne => "-1/-1",
    }
}

/// Projects a permanent's stored engine counters into the wire [`Counter`] list.
///
/// Ordering follows the permanent's `BTreeMap<CounterKind, _>` iteration, which
/// is sorted by [`CounterKind`] and therefore stable across runs. Absent kinds
/// are simply not emitted, so a permanent with no counters yields an empty
/// `Vec` (the `skip_serializing_if` wire shape stays unchanged).
fn permanent_counters(perm: &rune_engine::Permanent) -> Vec<Counter> {
    perm.counters
        .iter()
        .map(|(&kind, &count)| Counter {
            kind: counter_kind_str(kind).to_owned(),
            count,
        })
        .collect()
}

/// The opaque protocol id for an object on the stack.
fn stack_entity_id(id: StackId) -> String {
    format!("stack_{}", id.0)
}

/// Saturating `usize`→`u32` for wire counts; avoids both a panic and a lossy cast.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Map the engine's turn [`Step`] onto the protocol [`Phase`]. The two enums are
/// deliberately decoupled (`rune-engine` never depends on `rune-protocol`), so the
/// mapping is written out here.
fn phase_of(step: Step) -> Phase {
    match step {
        Step::Untap => Phase::Untap,
        Step::Upkeep => Phase::Upkeep,
        Step::Draw => Phase::Draw,
        Step::PrecombatMain => Phase::PrecombatMain,
        Step::BeginCombat => Phase::BeginCombat,
        Step::DeclareAttackers => Phase::DeclareAttackers,
        Step::DeclareBlockers => Phase::DeclareBlockers,
        Step::CombatDamage => Phase::CombatDamage,
        Step::EndCombat => Phase::EndCombat,
        Step::PostcombatMain => Phase::PostcombatMain,
        Step::End => Phase::End,
        Step::Cleanup => Phase::Cleanup,
    }
}

/// The display name of a card, or a stable placeholder if the id is unknown.
fn card_name(card: CardId, db: &CardDatabase) -> String {
    db.card(card)
        .map(|data| data.name.clone())
        .unwrap_or_else(|| format!("Unknown card {}", card.0))
}

/// Build the full [`CardView`] for a card the viewer is entitled to see.
fn card_view(entity_id: String, card: CardId, db: &CardDatabase) -> CardView {
    match db.card(card) {
        Some(data) => full_card_view(entity_id, data),
        None => CardView {
            id: entity_id,
            name: format!("Unknown card {}", card.0),
            type_line: String::new(),
            mana_cost: None,
            oracle_text: String::new(),
            power: None,
            toughness: None,
        },
    }
}

/// Project engine [`CardData`] onto the wire [`CardView`]. Power/toughness become
/// strings so non-numeric values round-trip (`rune-protocol`); an empty mana cost
/// is elided rather than sent as `""`.
fn full_card_view(entity_id: String, data: &CardData) -> CardView {
    CardView {
        id: entity_id,
        name: data.name.clone(),
        type_line: data.type_line(),
        mana_cost: (!data.mana_cost.is_empty()).then(|| data.mana_cost.clone()),
        oracle_text: data.oracle_text.clone(),
        power: data.power.map(|p| p.to_string()),
        toughness: data.toughness.map(|t| t.to_string()),
    }
}

/// A short human description of an ability's effects, for the stack view.
fn ability_description(effects: &[Effect]) -> String {
    let parts: Vec<String> = effects
        .iter()
        .map(|effect| match effect {
            Effect::AddMana { color, amount } => format!("Add {} {}", amount, color.pip()),
            Effect::DrawCard { count } => format!("Draw {count} card(s)"),
            Effect::Tap { .. } => "Tap target".to_string(),
        })
        .collect();
    if parts.is_empty() {
        "Ability".to_string()
    } else {
        parts.join(", ")
    }
}

/// Project one engine [`StackObject`] onto its wire [`StackItem`].
fn stack_item(object: &StackObject, db: &CardDatabase) -> StackItem {
    match &object.kind {
        StackObjectKind::Spell { card } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: card_name(card.card, db),
            source: None,
        },
        StackObjectKind::Ability { source, effects } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: ability_description(effects),
            source: Some(permanent_entity_id(*source)),
        },
    }
}

/// Build the [`ZonePile`]s for a public per-player pile (graveyard or exile),
/// skipping empty piles so the wire stays terse.
fn zone_piles(
    state: &GameState,
    pick: impl Fn(&Player) -> &Vec<CardInstance>,
    db: &CardDatabase,
) -> Vec<ZonePile> {
    state
        .players
        .iter()
        .enumerate()
        .filter_map(|(seat, player)| {
            let cards = pick(player);
            if cards.is_empty() {
                return None;
            }
            Some(ZonePile {
                player_id: player_id(PlayerId(seat)),
                cards: cards
                    .iter()
                    .map(|&inst| card_view(card_entity_id(inst.id), inst.card, db))
                    .collect(),
            })
        })
        .collect()
}

/// The actions the engine currently offers the priority holder, each paired with
/// the opaque id a client echoes back to choose it.
///
/// The ids are positional, but they are no longer what *binds* a returned answer to
/// an action: each projected [`ValidAction`] also carries a content-binding
/// [`token`](ValidAction::token) (see [`valid_action_view`]) hashed from the
/// action's own content. [`resolve_action`] verifies that token, so a stale
/// positional id whose action has since changed cannot silently rebind — the
/// full-state invariant now covers routing *and* content. Empty when no one holds
/// priority.
fn issued_actions(state: &GameState, db: &CardDatabase) -> Vec<(String, Action)> {
    valid_actions(state, db)
        .into_iter()
        .enumerate()
        .map(|(index, action)| (format!("a{index}"), action))
        .collect()
}

/// The content-binding token for an action, hashed from the exact content the
/// client is answering: its `kind`, `subject`, and `requirements` (slot ids,
/// prompts, and legal candidate entity ids). ADR 0009 §Protocol specifies a
/// hash/echo of the content — not a random nonce — so the server stays stateless:
/// it never stores a per-id secret, it recomputes the token from the freshly
/// regenerated action. Two actions with different content therefore hash to
/// different tokens, which is what lets [`resolve_action`] reject a stale or
/// redirected id whose token no longer matches.
fn content_token(kind: &str, subject: &[String], requirements: &[TargetRequirement]) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut hasher);
    subject.hash(&mut hasher);
    // `TargetRequirement` intentionally does not derive `Hash` (it is a wire type),
    // so fold its fields in explicitly, length-prefixed to stay unambiguous.
    requirements.len().hash(&mut hasher);
    for req in requirements {
        req.slot.hash(&mut hasher);
        req.prompt.hash(&mut hasher);
        req.candidates.hash(&mut hasher);
    }
    format!("t{:016x}", hasher.finish())
}

/// Project one engine [`Action`] onto its wire [`ValidAction`], attaching the
/// subject entity so the client can render the action on the card/permanent it
/// belongs to (ADR 0004), the ordered target `requirements` it must fill, and the
/// content-binding `token` (see [`content_token`]) the client echoes back.
///
/// Every subject/candidate names a *specific* game object by its per-instance id
/// ([`card_entity_id`]/[`permanent_entity_id`]/[`player_id`], issue #51), never a
/// bare printed card, so a targeted answer is unambiguous.
///
/// The engine [`Action`] set does not yet carry selectable targets (issues #70/#71
/// grow that), so `requirements` is empty for every current action and the token
/// binds `kind` + `subject` alone. When a targeted action lands, its target specs
/// are projected into `requirements` here — one slot per target, each listing the
/// engine's legal candidate entity ids — and the token binds them automatically.
fn valid_action_view(id: String, action: &Action, db: &CardDatabase) -> ValidAction {
    let (kind, label, subject, requirements): (
        String,
        String,
        Vec<String>,
        Vec<TargetRequirement>,
    ) = match action {
        Action::PassPriority => (
            "pass_priority".to_string(),
            "Pass priority".to_string(),
            Vec::new(),
            Vec::new(),
        ),
        Action::PlayLand { card } => (
            "play_land".to_string(),
            format!("Play {}", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            Vec::new(),
        ),
        Action::CastSpell { card } => (
            "cast_spell".to_string(),
            format!("Cast {}", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            Vec::new(),
        ),
        Action::Discard { card } => (
            "discard".to_string(),
            format!("Discard {}", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            Vec::new(),
        ),
        Action::ActivateAbility { permanent, .. } => (
            "activate_ability".to_string(),
            "Activate ability".to_string(),
            vec![permanent_entity_id(*permanent)],
            Vec::new(),
        ),
        // Pre-game London mulligan decisions (CR 103.5). Subject-less, so the
        // client renders them in the action bar (ADR 0004). The `Keep` bottoming
        // choice is a multi-select `requirements` slot (candidates = hand card
        // entity ids); projecting it rides the same engine→wire requirements
        // wiring still pending for targeting (ADR 0009 follow-up #73), so it is
        // left empty here for now — a first-hand keep (no bottoming) already works.
        Action::Mulligan => (
            "mulligan".to_string(),
            "Mulligan".to_string(),
            Vec::new(),
            Vec::new(),
        ),
        Action::Keep { .. } => (
            "keep".to_string(),
            "Keep hand".to_string(),
            Vec::new(),
            Vec::new(),
        ),
        // Combat declarations (CR 508/509) are subject-less choices offered to the
        // priority holder. Their multi-select candidate `requirements` (from the
        // engine's `attacker_candidates`/`blocker_candidates`) are projected once
        // the client-side declaration UX lands (a follow-up, mirroring how ability
        // target `requirements` are still deferred above); until then the empty
        // requirement form round-trips as a "no attackers/blockers" declaration.
        Action::DeclareAttackers { .. } => (
            "declare_attackers".to_string(),
            "Declare attackers".to_string(),
            Vec::new(),
            Vec::new(),
        ),
        Action::DeclareBlockers { .. } => (
            "declare_blockers".to_string(),
            "Declare blockers".to_string(),
            Vec::new(),
            Vec::new(),
        ),
    };
    let token = content_token(&kind, &subject, &requirements);
    ValidAction {
        id,
        kind,
        label,
        subject,
        requirements,
        token,
    }
}

/// Build the [`GameView`] the seat `viewer` is entitled to see.
///
/// Hidden information is redacted: the viewer receives full [`CardView`]s only for
/// their own hand and mana pool; every other seat is reduced to an
/// [`OpponentView`] of public counts (hand size, life, library and graveyard
/// sizes). Public state — battlefield, stack, graveyards, exile, phase — is shared
/// verbatim. `valid_actions` are populated only when `viewer` holds priority,
/// because the engine offers actions to exactly one seat at a time.
///
/// The view names its receiver in `you` (the viewer's `p{N}` seat id), so the
/// client identifies itself directly rather than inferring it from the zones.
///
/// A view is a complete snapshot: a client can reconstruct its entire UI from this
/// one message, which is what makes reconnect a plain re-send (`docs/protocol.md`).
pub(crate) fn personalized_view(
    state: &GameState,
    db: &CardDatabase,
    viewer: PlayerId,
) -> GameView {
    let my_hand = state
        .players
        .get(viewer.0)
        .map(|player| {
            player
                .hand
                .iter()
                .map(|&inst| card_view(card_entity_id(inst.id), inst.card, db))
                .collect()
        })
        .unwrap_or_default();

    let opponents = state
        .players
        .iter()
        .enumerate()
        .filter(|(seat, _)| *seat != viewer.0)
        .map(|(seat, player)| OpponentView {
            player_id: player_id(PlayerId(seat)),
            hand_size: count(player.hand.len()),
            life: player.life,
            library_size: count(player.library.len()),
            graveyard_size: count(player.graveyard.len()),
            statuses: Vec::new(),
        })
        .collect();

    let battlefield = state
        .battlefield
        .iter()
        .map(|perm| PermanentView {
            id: permanent_entity_id(perm.id),
            controller: player_id(perm.controller),
            // The engine models control but not separate ownership yet, so owner
            // mirrors controller until zone-ownership lands.
            owner: player_id(perm.controller),
            card: card_view(permanent_entity_id(perm.id), perm.card, db),
            tapped: perm.tapped,
            // Combat declaration state (CR 508/509): whether this permanent is
            // attacking, and which attacker it is blocking (as an entity id).
            attacking: perm.attacking,
            blocking: perm.blocking.map(permanent_entity_id),
            // Marked combat damage (CR 120.3 / 510), for lethal-damage display.
            damage: perm.damage,
            counters: permanent_counters(perm),
        })
        .collect();

    let stack = state.stack.iter().map(|o| stack_item(o, db)).collect();
    let graveyards = zone_piles(state, |p| &p.graveyard, db);
    let exile = zone_piles(state, |p| &p.exile, db);

    let mana_pool = state
        .players
        .get(viewer.0)
        .map(|player| player.mana_pool.pips())
        .unwrap_or_default();

    let holds_priority = state.priority_holder().is_some();
    let priority_player = holds_priority.then(|| player_id(state.priority));

    let valid_actions = if holds_priority && state.priority == viewer {
        issued_actions(state, db)
            .into_iter()
            .map(|(id, action)| valid_action_view(id, &action, db))
            .collect()
    } else {
        Vec::new()
    };

    GameView {
        you: player_id(viewer),
        my_hand,
        opponents,
        battlefield,
        stack,
        graveyards,
        exile,
        phase: phase_of(state.step),
        mana_pool,
        priority_player,
        valid_actions,
        action_deadline: None,
    }
}

/// Whether a returned target selection exactly fills an action's requirement
/// slots from their advertised legal candidates (ADR 0009 §Enumeration).
///
/// The check is against the *freshly recomputed* candidate sets, not the ones the
/// client saw: there must be exactly one [`TargetChoice`] per [`TargetRequirement`]
/// slot, each choice non-empty and drawn entirely from that slot's current legal
/// candidates. A redirected id therefore cannot smuggle in a target that is no
/// longer legal. Requirement-less actions accept exactly an empty selection.
fn targets_fill_requirements(targets: &[TargetChoice], requirements: &[TargetRequirement]) -> bool {
    if targets.len() != requirements.len() {
        return false;
    }
    requirements.iter().all(|req| {
        targets.iter().any(|choice| {
            choice.slot == req.slot
                && !choice.chosen.is_empty()
                && choice.chosen.iter().all(|id| req.candidates.contains(id))
        })
    })
}

/// Resolve a returned [`ChooseAction`] into the engine [`Action`] to apply, or
/// `None` if the answer does not name — and correctly bind to — an action this
/// `seat` was actually offered.
///
/// This is pure routing, not rules: the engine already decided legality (in
/// [`issued_actions`]); this only checks the answer against what was offered.
/// Because the engine offers actions to exactly one seat (the priority holder), an
/// answer from any other seat resolves to `None`. Resolution rejects, rather than
/// applies, when:
///
/// - the seat does not hold priority, or the id names no offered action;
/// - the returned [`token`](ChooseAction::token) is present but does not match the
///   token the server currently issues for that id — a stale/redirected id whose
///   action content has changed hashes to a different token, so it can never rebind
///   to a *different* action (ADR 0009 §Protocol, content binding);
/// - the token is absent (`""`) yet the offered action is a multi-step one that
///   *requires* binding — a bound action must be answered with its token, never on
///   the legacy positional path;
/// - the returned targets do not exactly fill the offered action's requirement
///   slots from their current legal candidate sets.
///
/// An empty token is still accepted for a plain, requirement-less action so the
/// terminal client (`rune-cli`), which does not yet echo tokens, keeps working;
/// such sequential actions are safe on the positional path (ADR 0009 §Context).
pub(crate) fn resolve_action(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    choice: &ChooseAction,
) -> Option<Action> {
    if state.priority_holder().is_none() || state.priority != seat {
        return None;
    }

    // Regenerate the offered action for this id and project it, so the token and
    // requirement candidates are recomputed from current state (stateless routing).
    let (action, offered) = issued_actions(state, db)
        .into_iter()
        .find(|(id, _)| *id == choice.action_id)
        .map(|(id, action)| {
            let offered = valid_action_view(id, &action, db);
            (action, offered)
        })?;

    // Content binding: verify the token (or, for a token-less answer, permit only a
    // plain action on the legacy positional path).
    if choice.token.is_empty() {
        if !offered.requirements.is_empty() {
            return None;
        }
    } else if choice.token != offered.token {
        return None;
    }

    // Validate the returned selection against the action's current legal candidates.
    if !targets_fill_requirements(&choice.targets, &offered.requirements) {
        return None;
    }

    // Map the validated selection onto the engine action. No engine `Action` carries
    // targets yet (issues #70/#71), so a well-formed answer has no targets to thread
    // in and the action resolves as-is; the chosen ids are applied here once the
    // engine grows targeted actions.
    Some(action)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// Build the [`ChooseAction`] a well-behaved client sends for `action`:
    /// echoing its id and content-binding token verbatim, with no targets (no
    /// current engine action carries requirements).
    fn answer(action: &ValidAction) -> ChooseAction {
        ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: Vec::new(),
        }
    }

    /// A `PrecombatMain` two-player state with the given hand for seat 0, who holds
    /// priority and can act at sorcery speed.
    fn state_with_hand(cards: &[CardId]) -> (GameState, Vec<CardInstance>) {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let hand: Vec<CardInstance> = cards.iter().map(|&c| state.new_instance(c)).collect();
        state.players[0].hand = hand.clone();
        (state, hand)
    }

    /// Two copies of the same printed card in one hand must project to distinct
    /// entity ids and independently routable actions (issue #51). Before
    /// per-instance identity both copies shared `card_5`, so a returned action
    /// resolved against "the first matching copy".
    #[test]
    fn issue_51_duplicate_hand_cards_get_distinct_entities_and_actions() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest_a = state.new_instance(CardId(5));
        let forest_b = state.new_instance(CardId(5));
        state.players[0].hand = vec![forest_a, forest_b];

        let view = personalized_view(&state, &db, PlayerId(0));

        // Each physical copy gets its own hand entity id.
        let hand_ids: Vec<&str> = view.my_hand.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(hand_ids.len(), 2);
        assert_ne!(hand_ids[0], hand_ids[1]);
        assert!(hand_ids.contains(&card_entity_id(forest_a.id).as_str()));
        assert!(hand_ids.contains(&card_entity_id(forest_b.id).as_str()));

        // Two land actions, each carrying its own copy's entity id as subject.
        let land_actions: Vec<&ValidAction> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "play_land")
            .collect();
        assert_eq!(land_actions.len(), 2);
        let subjects: Vec<&str> = land_actions.iter().map(|a| a.subject[0].as_str()).collect();
        assert_ne!(subjects[0], subjects[1]);

        // Each action id routes back to a PlayLand naming the exact instance its
        // subject entity referenced — no ambiguity, no "first matching copy".
        for action in &land_actions {
            let resolved = resolve_action(&state, &db, PlayerId(0), &answer(action)).unwrap();
            let Action::PlayLand { card } = resolved else {
                panic!("play_land action must resolve to a PlayLand");
            };
            assert_eq!(action.subject[0], card_entity_id(card.id));
        }

        // The two actions route to two different instances between them.
        let routed: Vec<CardInstance> = land_actions
            .iter()
            .map(
                |a| match resolve_action(&state, &db, PlayerId(0), &answer(a)).unwrap() {
                    Action::PlayLand { card } => card,
                    other => panic!("expected PlayLand, got {other:?}"),
                },
            )
            .collect();
        assert_ne!(routed[0].id, routed[1].id);
        assert!(routed.contains(&forest_a));
        assert!(routed.contains(&forest_b));
    }

    /// The engine RNG seed must never surface in a personalized view: it would
    /// leak future shuffle outcomes. Two states differing only in `rng_seed`
    /// therefore project to byte-identical views for the same seat.
    #[test]
    fn rng_seed_never_appears_in_a_personalized_view() {
        let db = CardDatabase::bundled().unwrap();
        let base = GameState::new_two_player_with_seed(0);
        let reseeded = GameState::new_two_player_with_seed(0xABCD_1234_5678_9ABC);

        for seat in 0..base.players.len() {
            let viewer = PlayerId(seat);
            assert_eq!(
                personalized_view(&base, &db, viewer),
                personalized_view(&reseeded, &db, viewer),
            );
        }
    }

    /// A battlefield permanent projects its stored engine counters into
    /// [`PermanentView::counters`] as `{ kind, count }` wire entries, in a
    /// deterministic order (sorted by [`CounterKind`], the map's key order), and
    /// a permanent with no counters projects to an empty list — which
    /// `skip_serializing_if` then drops from the JSON entirely (issue #68).
    #[test]
    fn issue_68_permanent_counters_project_into_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        // Seat 0 holds priority so the state is a valid, viewable snapshot.
        let with_counters = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: with_counters,
            instance: CardInstanceId(0),
            card: CardId(5),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage: 0,
            // Insertion order is deliberately reversed from the expected wire
            // order to prove the projection sorts by kind, not by insertion.
            counters: [
                (CounterKind::MinusOneMinusOne, 1),
                (CounterKind::PlusOnePlusOne, 2),
            ]
            .into_iter()
            .collect(),
        });
        let without_counters = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: without_counters,
            instance: CardInstanceId(1),
            card: CardId(5),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
        });

        let view = personalized_view(&state, &db, PlayerId(0));

        let counted = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(with_counters))
            .expect("permanent with counters must appear in the view");
        assert_eq!(
            counted.counters,
            vec![
                Counter {
                    kind: "+1/+1".into(),
                    count: 2,
                },
                Counter {
                    kind: "-1/-1".into(),
                    count: 1,
                },
            ],
            "counters must be sorted by kind (+1/+1 before -1/-1), not by insertion order",
        );

        let bare = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(without_counters))
            .expect("permanent without counters must appear in the view");
        assert!(
            bare.counters.is_empty(),
            "a permanent with no counters projects to an empty list",
        );

        // The empty list is dropped from the wire via `skip_serializing_if`, so
        // the serialized shape is unchanged from the always-empty placeholder.
        let json = serde_json::to_value(bare).unwrap();
        assert!(
            json.get("counters").is_none(),
            "empty counters must not be serialized (skip_serializing_if wire shape)",
        );
        let counted_json = serde_json::to_value(counted).unwrap();
        assert!(
            counted_json.get("counters").is_some(),
            "non-empty counters must be serialized",
        );
    }

    /// Combat declaration state is visible in the projected view (issue #117): an
    /// attacking permanent reports `attacking: true`, and a blocker reports the
    /// entity id of the attacker it is blocking. A permanent not in combat reports
    /// neither.
    #[test]
    fn issue_117_attack_and_block_state_project_into_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        let attacker = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: attacker,
            instance: CardInstanceId(0),
            card: CardId(6),
            controller: PlayerId(0),
            tapped: true,
            entered_turn: 0,
            attacking: true,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
        });
        let blocker = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: blocker,
            instance: CardInstanceId(1),
            card: CardId(6),
            controller: PlayerId(1),
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: Some(attacker),
            damage: 0,
            counters: std::collections::BTreeMap::new(),
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let attacker_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(attacker))
            .expect("attacker in view");
        assert!(attacker_view.attacking);
        assert_eq!(attacker_view.blocking, None);

        let blocker_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(blocker))
            .expect("blocker in view");
        assert!(!blocker_view.attacking);
        assert_eq!(
            blocker_view.blocking.as_deref(),
            Some(permanent_entity_id(attacker).as_str())
        );
    }

    /// Marked combat damage (issue #118) projects onto [`PermanentView::damage`]:
    /// a damaged permanent reports its marked damage, and an undamaged one reports
    /// `0`, which `skip_serializing_if` then drops from the wire.
    #[test]
    fn issue_118_marked_damage_projects_into_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        let damaged = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: damaged,
            instance: CardInstanceId(0),
            card: CardId(1),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage: 2,
            counters: std::collections::BTreeMap::new(),
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let projected = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(damaged))
            .expect("damaged permanent in view");
        assert_eq!(projected.damage, 2);

        // Zero marked damage elides from the JSON (skip_serializing_if wire shape).
        let mut undamaged = projected.clone();
        undamaged.damage = 0;
        let json = serde_json::to_value(&undamaged).unwrap();
        assert!(json.get("damage").is_none());
    }

    /// Every emitted action carries a non-empty content-binding token, and the
    /// token is a function of the action's content: two actions of the same kind
    /// that name different subjects hash to different tokens. This is what lets a
    /// stale positional id be caught when its action content changes.
    #[test]
    fn every_action_carries_a_content_bound_token() {
        let db = CardDatabase::bundled().unwrap();
        let (state, _) = state_with_hand(&[CardId(5), CardId(5)]);

        let view = personalized_view(&state, &db, PlayerId(0));
        assert!(view.valid_actions.iter().all(|a| !a.token.is_empty()));

        // Same `kind`, different subject instance -> different token.
        let land_tokens: Vec<&str> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "play_land")
            .map(|a| a.token.as_str())
            .collect();
        assert_eq!(land_tokens.len(), 2);
        assert_ne!(land_tokens[0], land_tokens[1]);

        // The token is deterministic: recomputing the same action reproduces it,
        // which is exactly what makes server-side verification stateless.
        let pass = &view.valid_actions[0];
        assert_eq!(
            pass.token,
            content_token(&pass.kind, &pass.subject, &pass.requirements),
        );
    }

    /// A token-bound action round-trips view -> choose -> engine: the client echoes
    /// the id and token it was issued and the server resolves it to the exact engine
    /// action, naming the specific instance the subject referenced.
    #[test]
    fn token_bound_action_round_trips_to_the_engine() {
        let db = CardDatabase::bundled().unwrap();
        let (state, hand) = state_with_hand(&[CardId(5)]);

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
        let (state, _) = state_with_hand(&[CardId(5)]);

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
        let (mut state, hand) = state_with_hand(&[CardId(5), CardId(5)]);
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
        let (state, _) = state_with_hand(&[CardId(5)]);

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
        let (state, _) = state_with_hand(&[CardId(5)]);

        let view = personalized_view(&state, &db, PlayerId(0));
        let pass = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass is always offered");

        let spurious = ChooseAction {
            action_id: pass.id.clone(),
            token: pass.token.clone(),
            targets: vec![TargetChoice {
                slot: "slot0".to_string(),
                chosen: vec![player_id(PlayerId(1))],
            }],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &spurious).is_none());
    }

    /// During the pre-game London mulligan (CR 103.5, engine issue #111) the view
    /// projects the deciding seat's keep/mulligan decision, each carrying a
    /// content-binding token (ADR 0009), while hand redaction is unaffected: the
    /// viewer sees its own hand in full and only the *size* of the opponent's.
    #[test]
    fn mulligan_actions_project_with_tokens_and_redaction_holds() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        // Enter the mulligan phase with seat 0 deciding; give both seats hands.
        state.players[0].hand = vec![state.new_instance(CardId(5))];
        state.players[1].hand = vec![state.new_instance(CardId(6)), state.new_instance(CardId(1))];
        state.mulligan = Some(rune_engine::MulliganState::new(2, 7));

        let view = personalized_view(&state, &db, PlayerId(0));

        // Redaction is unaffected: the viewer sees its own hand in full, and the
        // opponent is reduced to a hand *size* with no card contents leaked.
        assert_eq!(view.my_hand.len(), 1);
        assert_eq!(view.opponents.len(), 1);
        assert_eq!(view.opponents[0].hand_size, 2);

        // The deciding seat is offered exactly keep + mulligan, each token-bound.
        let kinds: Vec<&str> = view.valid_actions.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"mulligan"));
        assert!(kinds.contains(&"keep"));
        assert!(
            view.valid_actions.iter().all(|a| !a.token.is_empty()),
            "every mulligan action carries a content-binding token (ADR 0009)",
        );
        let mulligan = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "mulligan")
            .unwrap();
        let keep = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "keep")
            .unwrap();
        assert_ne!(
            mulligan.token, keep.token,
            "distinct action content hashes to distinct tokens",
        );

        // The non-deciding seat is offered nothing (actions are redacted to the
        // priority holder) and still only sees the opponent's hand size.
        let other = personalized_view(&state, &db, PlayerId(1));
        assert!(other.valid_actions.is_empty());
        assert_eq!(other.opponents[0].hand_size, 1);
    }
}
