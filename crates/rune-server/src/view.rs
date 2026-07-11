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
    attacker_candidates, blocker_candidates, bottom_requirement, declared_attackers,
    target_requirements, valid_actions, Action, Block, CardData, CardDatabase, CardId,
    CardInstance, CardInstanceId, CounterKind, Effect, GameResult, GameState, LossReason,
    PermanentId, Player, PlayerId, StackId, StackObject, StackObjectKind, Step, Target, TargetSpec,
};
use rune_protocol::{
    CardView, ChooseAction, Counter, GameOverReason, GameResult as GameResultView, GameView,
    OpponentView, Permanent as PermanentView, Phase, Prompt, PromptOption, StackItem, TargetChoice,
    TargetRequirement, ValidAction, ZonePile,
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
            Effect::CounterSpell { .. } => "Counter target spell".to_string(),
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

/// How a returned answer for a projected wire action is bound back onto a concrete
/// engine [`Action`]. Most wire actions are a 1:1 [`Bind::Standard`] projection of a
/// single engine action; two are *collapsed* projections that fold a combinatorial
/// engine enumeration into one richer-prompt action (issue #156):
/// [`Bind::MulliganDecision`] replaces the separate `Mulligan`/`Keep` actions with a
/// single [`Prompt::Option`], and [`Bind::DiscardFromHand`] replaces the per-card
/// cleanup `Discard` actions with a single [`Prompt::SelectFromZone`].
enum Bind {
    /// A 1:1 projection of this engine action; resolution threads any target
    /// `requirements` back through the per-kind `bind_*` helpers.
    Standard(Action),
    /// The collapsed mulligan keep/take-another decision: an [`Prompt::Option`] plus,
    /// when a bottoming is owed, the [`bottom_requirement`] slot (CR 103.5).
    MulliganDecision,
    /// The collapsed cleanup discard: a single [`Prompt::SelectFromZone`] over the
    /// active player's hand, resolving to one [`Action::Discard`] (CR 514.1).
    DiscardFromHand,
}

/// One projected wire action together with how to bind a returned answer to it.
struct Projected {
    /// The wire action the client sees and answers.
    view: ValidAction,
    /// How [`resolve_action`] maps the answer back onto an engine [`Action`].
    bind: Bind,
}

/// The wire actions the engine currently offers the priority holder, each paired
/// with how a returned answer binds back to the engine.
///
/// The ids are positional (`a0`, `a1`, …), but they are no longer what *binds* a
/// returned answer to an action: each projected [`ValidAction`] also carries a
/// content-binding [`token`](ValidAction::token) hashed from the action's own
/// content (kind + subject + requirements + prompts). [`resolve_action`] verifies
/// that token, so a stale positional id whose action has since changed cannot
/// silently rebind. Empty when no one holds priority.
///
/// Two engine enumerations are *collapsed* into one richer-prompt action apiece
/// (issue #156), deleting the enumeration: the pre-game `Mulligan`/`Keep` pair
/// becomes a single `mulligan_decision` (an [`Prompt::Option`]), and the per-card
/// cleanup `Discard` list becomes a single `discard` (a [`Prompt::SelectFromZone`]).
/// Every other engine action projects 1:1 via [`valid_action_view`].
fn projected_actions(state: &GameState, db: &CardDatabase) -> Vec<Projected> {
    let mut out: Vec<Projected> = Vec::new();
    let mut next = 0usize;
    let mut mulligan_done = false;
    let mut discard_done = false;
    for action in valid_actions(state, db) {
        let projected = match &action {
            // Collapse the keep/take-another pair into one option-bearing action.
            Action::Mulligan | Action::Keep { .. } => {
                if mulligan_done {
                    continue;
                }
                mulligan_done = true;
                build_mulligan_decision(state, next_id(&mut next))
            }
            // Collapse the per-card discard list into one select-from-zone action.
            Action::Discard { .. } => {
                if discard_done {
                    continue;
                }
                discard_done = true;
                build_discard(state, next_id(&mut next))
            }
            _ => Projected {
                view: valid_action_view(next_id(&mut next), &action, state, db),
                bind: Bind::Standard(action),
            },
        };
        out.push(projected);
    }
    out
}

/// Take the next positional wire id (`a0`, `a1`, …), advancing the counter. Only
/// called when an action is actually emitted, so ids stay dense across collapses.
fn next_id(next: &mut usize) -> String {
    let id = format!("a{next}");
    *next += 1;
    id
}

/// The collapsed mulligan keep/take-another decision (CR 103.5, London), a real
/// [`Prompt::Option`] projection (issue #156). The two engine actions
/// [`Action::Mulligan`]/[`Action::Keep`] are folded into one `mulligan_decision`
/// action carrying an option slot (`decision`) whose two choices are *keep* and
/// *mulligan*. When a bottoming is owed (the seat has mulliganed), the same action
/// also carries the [`bottom_requirement`] multi-select slot from issue #140, so a
/// keep answer selects which cards to bottom; [`resolve_action`] binds *keep* to
/// [`Action::Keep`] with those cards and *mulligan* to [`Action::Mulligan`].
fn build_mulligan_decision(state: &GameState, id: String) -> Projected {
    let kind = "mulligan_decision".to_string();
    let subject: Vec<String> = Vec::new();
    // The bottoming is projected exactly as issue #140 did — as a `requirements`
    // multi-select slot — so a keep still binds through [`bind_keep`] unchanged.
    let requirements = keep_requirements(state, &Action::Keep { bottom: Vec::new() });
    let prompts = vec![Prompt::Option {
        slot: "decision".to_string(),
        prompt: "Keep this hand or take a mulligan?".to_string(),
        options: vec![
            PromptOption {
                id: "keep".to_string(),
                label: "Keep this hand".to_string(),
            },
            PromptOption {
                id: "mulligan".to_string(),
                label: "Mulligan".to_string(),
            },
        ],
    }];
    let token = content_token(&kind, &subject, &requirements, &prompts);
    Projected {
        view: ValidAction {
            id,
            kind,
            label: "Keep or mulligan".to_string(),
            subject,
            requirements,
            prompts,
            token,
        },
        bind: Bind::MulliganDecision,
    }
}

/// The collapsed cleanup discard-to-maximum choice (CR 514.1), a real
/// [`Prompt::SelectFromZone`] projection (issue #156). The engine offers one
/// [`Action::Discard`] per card in the over-full hand; this folds them into a single
/// `discard` action carrying one select-from-zone slot over the active player's hand
/// (`count: 1` — the engine discards one card per turn-based check, re-offering while
/// still over the limit). [`resolve_action`] binds the chosen id to that
/// [`Action::Discard`].
fn build_discard(state: &GameState, id: String) -> Projected {
    let seat = state.priority;
    let candidates: Vec<String> = state
        .players
        .get(seat.0)
        .map(|player| {
            player
                .hand
                .iter()
                .map(|inst| card_entity_id(inst.id))
                .collect()
        })
        .unwrap_or_default();
    let kind = "discard".to_string();
    let subject: Vec<String> = Vec::new();
    let requirements: Vec<TargetRequirement> = Vec::new();
    let prompts = vec![Prompt::SelectFromZone {
        slot: "discard".to_string(),
        prompt: "Choose a card to discard".to_string(),
        zone: "hand".to_string(),
        owner: player_id(seat),
        count: 1,
        candidates,
    }];
    let token = content_token(&kind, &subject, &requirements, &prompts);
    Projected {
        view: ValidAction {
            id,
            kind,
            label: "Discard a card".to_string(),
            subject,
            requirements,
            prompts,
            token,
        },
        bind: Bind::DiscardFromHand,
    }
}

/// The content-binding token for an action, hashed from the exact content the
/// client is answering: its `kind`, `subject`, `requirements` (target slots), and
/// `prompts` (the option/select-from-zone/order slots, issue #156). ADR 0009
/// §Protocol specifies a hash/echo of the content — not a random nonce — so the
/// server stays stateless: it never stores a per-id secret, it recomputes the token
/// from the freshly regenerated action. Two actions with different content therefore
/// hash to different tokens, which is what lets [`resolve_action`] reject a stale or
/// redirected id whose token no longer matches — for a prompt-bearing action just as
/// for a targeted one.
fn content_token(
    kind: &str,
    subject: &[String],
    requirements: &[TargetRequirement],
    prompts: &[Prompt],
) -> String {
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
    // `Prompt` is likewise a non-`Hash` wire enum; fold each variant's tag and fields
    // in explicitly so a change to any prompt content re-derives a different token.
    prompts.len().hash(&mut hasher);
    for prompt in prompts {
        hash_prompt(prompt, &mut hasher);
    }
    format!("t{:016x}", hasher.finish())
}

/// Fold one wire [`Prompt`] into `hasher` for [`content_token`]: a per-variant tag
/// byte followed by its fields, length-prefixed where variable, so two prompts that
/// differ anywhere hash differently.
fn hash_prompt(prompt: &Prompt, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    match prompt {
        Prompt::Option {
            slot,
            prompt,
            options,
        } => {
            0u8.hash(hasher);
            slot.hash(hasher);
            prompt.hash(hasher);
            options.len().hash(hasher);
            for option in options {
                option.id.hash(hasher);
                option.label.hash(hasher);
            }
        }
        Prompt::SelectFromZone {
            slot,
            prompt,
            zone,
            owner,
            count,
            candidates,
        } => {
            1u8.hash(hasher);
            slot.hash(hasher);
            prompt.hash(hasher);
            zone.hash(hasher);
            owner.hash(hasher);
            count.hash(hasher);
            candidates.hash(hasher);
        }
        Prompt::Order {
            slot,
            prompt,
            items,
        } => {
            2u8.hash(hasher);
            slot.hash(hasher);
            prompt.hash(hasher);
            items.hash(hasher);
        }
    }
}

/// The opaque wire entity id naming the specific game object an engine [`Target`]
/// points at, reusing the same per-instance id scheme every other action uses
/// ([`card_entity_id`]/[`permanent_entity_id`]/[`player_id`]). This is what makes a
/// projected candidate — and a returned selection — name one unambiguous object.
fn target_entity_id(target: Target) -> String {
    match target {
        Target::Player(seat) => player_id(seat),
        Target::Permanent(id) => permanent_entity_id(id),
        Target::Card(id) => card_entity_id(id),
        Target::Spell(id) => stack_entity_id(id),
    }
}

/// The human-readable prompt for an ability-target slot's [`TargetSpec`]. Kept
/// exhaustive so a new spec forces a matching wire prompt here.
fn target_spec_prompt(spec: TargetSpec) -> &'static str {
    match spec {
        TargetSpec::AnyPlayer => "Choose target player",
        TargetSpec::AnyPermanent => "Choose target permanent",
        TargetSpec::AnyCreature => "Choose target creature",
        TargetSpec::SpellOnStack => "Choose target spell",
    }
}

/// The stable requirement-slot id for the blockers assigned to `attacker` in a
/// [`Action::DeclareBlockers`] projection. One slot per declared attacker, keyed by
/// the attacker's permanent id, so the returned choice names which attacker the
/// selected blockers are assigned to. Recomputed (never parsed) on resolution.
fn blocker_slot(attacker: PermanentId) -> String {
    format!("block_{}", attacker.0)
}

/// The bottoming requirement slot for a mulligan [`Action::Keep`] (CR 103.5,
/// London): the [`bottom_requirement`] candidates (the deciding seat's hand cards)
/// projected as a single multi-select slot asking for `count` cards. Empty for a
/// first-hand keep (nothing owed), so that keep stays a plain, choice-free action.
fn keep_requirements(state: &GameState, action: &Action) -> Vec<TargetRequirement> {
    match bottom_requirement(state, action) {
        Some(req) => vec![TargetRequirement {
            slot: "bottom".to_string(),
            prompt: format!("Put {} card(s) on the bottom of your library", req.count),
            candidates: req.candidates.into_iter().map(target_entity_id).collect(),
        }],
        None => Vec::new(),
    }
}

/// The attacker-declaration requirement slot (CR 508.1a): the engine's
/// [`attacker_candidates`] projected as a single multi-select slot. Empty when no
/// creature may attack, so declaring no attackers stays a plain, choice-free action
/// (and the token-less path keeps working when there is nothing to choose).
fn attacker_requirements(state: &GameState, db: &CardDatabase) -> Vec<TargetRequirement> {
    let candidates = attacker_candidates(state, db);
    if candidates.is_empty() {
        return Vec::new();
    }
    vec![TargetRequirement {
        slot: "attackers".to_string(),
        prompt: "Choose which creatures attack".to_string(),
        candidates: candidates.into_iter().map(permanent_entity_id).collect(),
    }]
}

/// The blocker-declaration requirement slots (CR 509.1a): one slot per declared
/// attacker ([`declared_attackers`]), each listing the eligible
/// [`blocker_candidates`] the defender may assign to that attacker. Empty when
/// there is nothing to block or no creature to block with, so declaring no blockers
/// stays a plain, choice-free action.
fn blocker_requirements(state: &GameState, db: &CardDatabase) -> Vec<TargetRequirement> {
    let attackers = declared_attackers(state);
    let blockers = blocker_candidates(state, db);
    if attackers.is_empty() || blockers.is_empty() {
        return Vec::new();
    }
    let candidates: Vec<String> = blockers.into_iter().map(permanent_entity_id).collect();
    attackers
        .into_iter()
        .map(|attacker| TargetRequirement {
            slot: blocker_slot(attacker),
            prompt: format!(
                "Choose blockers for {}",
                permanent_card_name(state, attacker, db)
            ),
            candidates: candidates.clone(),
        })
        .collect()
}

/// The ability-target requirement slots (ADR 0009 §Enumeration, deferral #73): the
/// engine's per-slot [`target_requirements`] candidate sets projected one slot each
/// (`t0`, `t1`, …), reusing the same content-binding machinery as the mulligan and
/// combat multi-selects. Empty for a non-targeting ability.
fn ability_requirements(
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
            candidates: req.candidates.into_iter().map(target_entity_id).collect(),
        })
        .collect()
}

/// The display name of the permanent `id` on the battlefield, for a human prompt,
/// or a stable placeholder if it is not found.
fn permanent_card_name(state: &GameState, id: PermanentId, db: &CardDatabase) -> String {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .map(|perm| card_name(perm.card, db))
        .unwrap_or_else(|| "the attacker".to_string())
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
/// Multi-select and targeted actions carry their engine candidate sets in
/// `requirements`, projected from the freshly computed legal sets (issue #140):
/// the mulligan [`Action::Keep`] bottoming ([`bottom_requirement`]), the combat
/// [`Action::DeclareAttackers`]/[`Action::DeclareBlockers`] declarations
/// ([`attacker_candidates`]/[`blocker_candidates`]), and ability targets
/// ([`target_requirements`], ADR 0009 deferral #73). The token binds those
/// requirements automatically (see [`content_token`]), and [`resolve_action`] maps
/// a returned selection back onto the concrete engine action. An action with
/// nothing to choose projects empty `requirements` and stays a plain action.
fn valid_action_view(
    id: String,
    action: &Action,
    state: &GameState,
    db: &CardDatabase,
) -> ValidAction {
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
        // A cast's target requirements (CR 601.2c) come from the same per-slot
        // enumeration abilities use ([`target_requirements`]); an untargeted spell
        // projects none. Wiring the returned selection back into a targeted cast is
        // a later server slice (ADR 0009 §Client / #73) — the engine already
        // records and re-checks the targets.
        Action::CastSpell { card, .. } => (
            "cast_spell".to_string(),
            format!("Cast {}", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            ability_requirements(state, db, action),
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
            ability_requirements(state, db, action),
        ),
        // Pre-game London mulligan decisions (CR 103.5). Subject-less, so the
        // client renders them in the action bar (ADR 0004). A `Mulligan` has no
        // sub-choice; a `Keep` carries the bottoming multi-select slot (candidates
        // = the deciding seat's hand card entity ids, count = mulligans taken) when
        // one is owed, and nothing for a first-hand keep.
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
            keep_requirements(state, action),
        ),
        // Combat declarations (CR 508/509) are subject-less choices offered to the
        // priority holder, carrying their multi-select candidate `requirements` from
        // the engine's freshly computed legal sets: attacker candidates for the
        // active player, and one blocker slot per declared attacker for the
        // defender. Empty when there is nothing to declare, so the empty (token-less)
        // form still round-trips as a "no attackers/blockers" declaration.
        Action::DeclareAttackers { .. } => (
            "declare_attackers".to_string(),
            "Declare attackers".to_string(),
            Vec::new(),
            attacker_requirements(state, db),
        ),
        Action::DeclareBlockers { .. } => (
            "declare_blockers".to_string(),
            "Declare blockers".to_string(),
            Vec::new(),
            blocker_requirements(state, db),
        ),
        // Concede (CR 104.3a): a subject-less action always offered to the acting
        // seat, rendered in the action bar (ADR 0004).
        Action::Concede => (
            "concede".to_string(),
            "Concede".to_string(),
            Vec::new(),
            Vec::new(),
        ),
    };
    // A 1:1 engine-action projection carries no `prompts` (the option /
    // select-from-zone / order shapes ride only on the collapsed mulligan and
    // discard actions, issue #156).
    let prompts: Vec<Prompt> = Vec::new();
    let token = content_token(&kind, &subject, &requirements, &prompts);
    ValidAction {
        id,
        kind,
        label,
        subject,
        requirements,
        prompts,
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
        projected_actions(state, db)
            .into_iter()
            .map(|projected| projected.view)
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
        // The terminal result once the game is over (CR 104.2a); `None` — and so
        // omitted from the wire — while the game is live.
        result: state.result().map(result_view),
    }
}

/// The wire name for an engine [`LossReason`], as the client expects it in
/// [`GameOverReason`]. Kept exhaustive so a new engine reason forces a matching
/// wire variant here rather than silently going unnamed.
fn game_over_reason(reason: LossReason) -> GameOverReason {
    match reason {
        LossReason::ZeroLife => GameOverReason::LifeZero,
        LossReason::DrewFromEmptyLibrary => GameOverReason::Decked,
        LossReason::Concede => GameOverReason::Concede,
    }
}

/// Project the engine's terminal [`GameResult`] onto the wire [`GameResultView`],
/// naming each seat by its `p{N}` id (CR 104.2a). Pure translation, no game logic.
fn result_view(result: GameResult) -> GameResultView {
    GameResultView {
        winner: result.winner.map(player_id),
        losers: result.losers.into_iter().map(player_id).collect(),
        reason: game_over_reason(result.reason),
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

/// The entity ids chosen for `slot` in a returned selection, or an empty slice if
/// the client sent no answer for it (a legal "select nothing" for an optional
/// multi-select like a combat declaration).
fn chosen_for<'a>(targets: &'a [TargetChoice], slot: &str) -> &'a [String] {
    targets
        .iter()
        .find(|choice| choice.slot == slot)
        .map_or(&[], |choice| choice.chosen.as_slice())
}

/// Map a returned mulligan bottoming selection onto the concrete
/// [`Action::Keep`] (CR 103.5): each chosen entity id must name a card currently in
/// the deciding seat's hand, resolved to its [`Target::Card`]. `None` if any chosen
/// id names no such card (rejecting the answer rather than silently dropping it).
fn bind_keep(state: &GameState, targets: &[TargetChoice]) -> Option<Action> {
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
fn bind_attackers(
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
    let mut attackers = Vec::new();
    for id in chosen_for(targets, "attackers") {
        attackers.push(permanent_in(&candidates, id)?);
    }
    Some(Action::DeclareAttackers { attackers })
}

/// Map a returned blocker declaration onto the concrete
/// [`Action::DeclareBlockers`] (CR 509.1a): each answered slot names a declared
/// attacker, and every chosen id in it must be a current blocker candidate assigned
/// to that attacker. An empty selection — declare no blockers — is legal. Any slot
/// that names no declared attacker, or a non-candidate blocker, rejects the answer.
fn bind_blockers(state: &GameState, db: &CardDatabase, targets: &[TargetChoice]) -> Option<Action> {
    let attackers = declared_attackers(state);
    let candidates = blocker_candidates(state, db);
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
fn bind_ability_targets(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
    targets: &[TargetChoice],
) -> Option<Action> {
    let requirements = target_requirements(state, db, action);
    let mut chosen = Vec::with_capacity(requirements.len());
    for (index, req) in requirements.iter().enumerate() {
        let [id] = chosen_for(targets, &format!("t{index}")) else {
            return None;
        };
        let target = req
            .candidates
            .iter()
            .copied()
            .find(|&candidate| target_entity_id(candidate) == *id)?;
        chosen.push(target);
    }
    match action {
        Action::ActivateAbility {
            permanent, index, ..
        } => Some(Action::ActivateAbility {
            permanent: *permanent,
            index: *index,
            targets: chosen,
        }),
        Action::CastSpell { card, .. } => Some(Action::CastSpell {
            card: *card,
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

/// Resolve a returned [`ChooseAction`] into the engine [`Action`] to apply, or
/// `None` if the answer does not name — and correctly bind to — an action this
/// `seat` was actually offered.
///
/// This is pure routing, not rules: the engine already decided legality (in
/// [`projected_actions`]) and re-checks it in [`apply_action`](rune_engine::apply_action);
/// this only checks the answer against what was offered and threads the chosen
/// selection onto the concrete engine action. Because the engine offers actions to
/// exactly one seat (the priority holder), an answer from any other seat resolves to
/// `None`. Resolution rejects, rather than applies, when:
///
/// - the seat does not hold priority, or the id names no offered action;
/// - the returned [`token`](ChooseAction::token) is present but does not match the
///   token the server currently issues for that id — a stale/redirected id whose
///   action content has changed hashes to a different token, so it can never rebind
///   to a *different* action (ADR 0009 §Protocol, content binding);
/// - the token is absent (`""`) yet the offered action carries `requirements` and so
///   *requires* binding — a bound action must be answered with its token, never on
///   the legacy positional path;
/// - the returned selection does not map onto the offered action's requirement slots
///   from their current legal candidate sets (see the per-kind `bind_*` helpers).
///
/// The mulligan bottoming and ability-target slots are mandatory (each must be
/// filled from its candidates), while the combat declarations are optional
/// multi-selects — an empty selection legally declares no attackers/blockers. An
/// empty token is still accepted for a plain, requirement-less action (a
/// requirement-less combat declaration included), so the token-less positional path
/// keeps working for sequential actions (ADR 0009 §Context).
pub(crate) fn resolve_action(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    choice: &ChooseAction,
) -> Option<Action> {
    if state.priority_holder().is_none() || state.priority != seat {
        return None;
    }

    // Regenerate the offered wire actions from current state, so the token,
    // requirement candidates, and prompt content are all recomputed (stateless
    // routing), then find the one this answer names.
    let Projected {
        view: offered,
        bind,
    } = projected_actions(state, db)
        .into_iter()
        .find(|projected| projected.view.id == choice.action_id)?;

    // Content binding: verify the token (or, for a token-less answer, permit only a
    // plain action on the legacy positional path — one carrying neither requirements
    // nor prompts to bind).
    if choice.token.is_empty() {
        if !offered.requirements.is_empty() || !offered.prompts.is_empty() {
            return None;
        }
    } else if choice.token != offered.token {
        return None;
    }

    match bind {
        // The collapsed richer-prompt actions (issue #156) map their option /
        // select-from-zone answers back onto the concrete engine action.
        Bind::MulliganDecision => bind_mulligan_decision(state, &offered, &choice.targets),
        Bind::DiscardFromHand => bind_discard(state, &offered, &choice.targets),
        // A 1:1 engine-action projection. The combat declarations are optional
        // multi-selects (empty is legal), so they bind directly against their fresh
        // candidate sets; the ability targets are mandatory slots, gated by
        // [`targets_fill_requirements`] first.
        Bind::Standard(action) => match &action {
            Action::DeclareAttackers { .. } => {
                bind_attackers(state, db, &offered.requirements, &choice.targets)
            }
            Action::DeclareBlockers { .. } => bind_blockers(state, db, &choice.targets),
            Action::ActivateAbility { .. } | Action::CastSpell { .. } => {
                if !targets_fill_requirements(&choice.targets, &offered.requirements) {
                    return None;
                }
                bind_ability_targets(state, db, &action, &choice.targets)
            }
            _ => {
                if !targets_fill_requirements(&choice.targets, &offered.requirements) {
                    return None;
                }
                Some(action)
            }
        },
    }
}

/// Bind a returned answer to the collapsed `mulligan_decision` action (issue #156):
/// read the mandatory `decision` [`Prompt::Option`] and route *mulligan* to
/// [`Action::Mulligan`] or *keep* to [`Action::Keep`], threading any bottoming
/// selection through [`bind_keep`]. `None` if the option slot is unanswered, answered
/// with an unknown id, or (for a keep that owes a bottoming) the `bottom` slot is not
/// filled from its freshly recomputed candidates.
fn bind_mulligan_decision(
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
            // Any owed bottoming slot must be filled from its current candidates
            // (the extra `decision` prompt slot is ignored). The engine re-checks the
            // exact owed count in `apply_action` (CR 103.5).
            let bottoming_ok = offered.requirements.iter().all(|req| {
                targets.iter().any(|choice| {
                    choice.slot == req.slot
                        && !choice.chosen.is_empty()
                        && choice.chosen.iter().all(|id| req.candidates.contains(id))
                })
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
fn bind_discard(
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// A terminal game (issue #119) projects its result onto the view: the winner,
    /// losers, and reason are named, and `valid_actions` is empty (CR 104.2a).
    #[test]
    fn issue_119_terminal_result_projects_onto_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.players[1].has_lost = true;
        state.players[1].loss_reason = Some(LossReason::Concede);

        let view = personalized_view(&state, &db, PlayerId(0));
        let result = view.result.expect("a terminal state carries a result");
        assert_eq!(result.winner.as_deref(), Some("p0"));
        assert_eq!(result.losers, vec!["p1".to_string()]);
        assert_eq!(result.reason, GameOverReason::Concede);
        assert!(
            view.valid_actions.is_empty(),
            "a terminal state offers no actions (CR 104.2a)"
        );

        // A live game omits the result entirely.
        let live = personalized_view(&GameState::new_two_player(), &db, PlayerId(0));
        assert!(live.result.is_none());
    }

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
            content_token(&pass.kind, &pass.subject, &pass.requirements, &pass.prompts),
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

    /// During the pre-game London mulligan (CR 103.5) the view projects the deciding
    /// seat's keep/take-another choice as a single `mulligan_decision` action
    /// carrying an [`Prompt::Option`] (issue #156, the real `option` projection),
    /// token-bound, while hand redaction is unaffected: the viewer sees its own hand
    /// in full and only the *size* of the opponent's.
    #[test]
    fn mulligan_decision_projects_an_option_prompt_and_redaction_holds() {
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

        // The two engine actions collapse into ONE token-bound `mulligan_decision`
        // (plus the always-available concede) — the keep/mulligan enumeration is gone.
        assert!(
            view.valid_actions.iter().all(|a| !a.token.is_empty()),
            "every action carries a content-binding token (ADR 0009)",
        );
        assert!(view.valid_actions.iter().all(|a| a.kind != "keep"));
        assert!(view.valid_actions.iter().all(|a| a.kind != "mulligan"));
        let decision = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "mulligan_decision")
            .expect("the deciding seat is offered a single mulligan decision");

        // It carries exactly one `option` prompt whose choices are keep + mulligan.
        assert_eq!(decision.prompts.len(), 1);
        let Prompt::Option { slot, options, .. } = &decision.prompts[0] else {
            panic!("the mulligan decision is an option prompt");
        };
        assert_eq!(slot, "decision");
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            vec!["keep", "mulligan"],
        );
        // A first-hand keep owes no bottoming, so there is no select-from-zone slot.
        assert!(decision.requirements.is_empty());

        // Both options resolve back to the concrete engine actions.
        let keep = ChooseAction {
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["keep".to_string()],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &keep),
            Some(Action::Keep { bottom: Vec::new() }),
        );
        let mull = ChooseAction {
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["mulligan".to_string()],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &mull),
            Some(Action::Mulligan),
        );
        // An unknown option id is rejected.
        let bogus = ChooseAction {
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["scoop".to_string()],
            }],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &bogus).is_none());

        // The non-deciding seat is offered nothing (actions are redacted to the
        // priority holder) and still only sees the opponent's hand size.
        let other = personalized_view(&state, &db, PlayerId(1));
        assert!(other.valid_actions.is_empty());
        assert_eq!(other.opponents[0].hand_size, 1);
    }

    /// Put a creature (or any card) permanent onto the battlefield under
    /// `controller`, returning its fresh [`PermanentId`]. `attacking`/`tapped` let a
    /// caller stage a combat state directly.
    fn put_permanent(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        tapped: bool,
        attacking: bool,
    ) -> PermanentId {
        let id = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id,
            instance: CardInstanceId(0),
            card,
            controller,
            tapped,
            // Entered a previous turn, so it is free of summoning sickness in a
            // turn > 0 combat state (CR 302.6).
            entered_turn: 0,
            attacking,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
        });
        id
    }

    /// A mulligan decision taken after a mulligan carries, alongside its `option`
    /// slot, the London bottoming (CR 103.5) as a `select_from_zone` prompt over the
    /// hand's cards (issue #156, the `select_from_zone` projection reusing #140's
    /// bottoming), and a keep answer naming the owed cards resolves to a `Keep`
    /// bottoming exactly those cards.
    #[test]
    fn mulligan_decision_keep_projects_bottoming_as_select_from_zone_and_resolves() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let c0 = state.new_instance(CardId(5));
        let c1 = state.new_instance(CardId(6));
        state.players[0].hand = vec![c0, c1];
        state.players[1].hand = vec![state.new_instance(CardId(1))];
        // Seat 0 has taken one mulligan, so a keep now owes one bottomed card.
        let mut mull = rune_engine::MulliganState::new(2, 7);
        mull.decisions[0].taken = 1;
        state.mulligan = Some(mull);

        let view = personalized_view(&state, &db, PlayerId(0));
        let decision = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "mulligan_decision")
            .expect("the deciding seat is offered a mulligan decision");

        // The bottoming rides #140's `requirements` "bottom" slot (candidates = the
        // hand cards, count implied by the owed mulligans).
        assert_eq!(decision.requirements.len(), 1, "one bottoming slot");
        assert_eq!(decision.requirements[0].slot, "bottom");
        assert_eq!(
            decision.requirements[0].candidates,
            vec![card_entity_id(c0.id), card_entity_id(c1.id)],
        );

        // A keep naming one card to bottom resolves to a Keep bottoming exactly it.
        let choose = ChooseAction {
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![
                TargetChoice {
                    slot: "decision".to_string(),
                    chosen: vec!["keep".to_string()],
                },
                TargetChoice {
                    slot: "bottom".to_string(),
                    chosen: vec![card_entity_id(c0.id)],
                },
            ],
        };
        let resolved =
            resolve_action(&state, &db, PlayerId(0), &choose).expect("the selection resolves");
        assert_eq!(
            resolved,
            Action::Keep {
                bottom: vec![Target::Card(c0.id)],
            },
        );

        // A keep that omits the owed bottoming is rejected (the mandatory slot is
        // unfilled), so a stale/empty answer cannot bottom nothing when one is owed.
        let empty_keep = ChooseAction {
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["keep".to_string()],
            }],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &empty_keep).is_none());
    }

    /// A `PrecombatMain`-agnostic cleanup state (CR 514.1) with the active player
    /// over the maximum hand size, for the discard `select_from_zone` projection.
    fn cleanup_over_hand_limit() -> (GameState, Vec<CardInstance>) {
        let mut state = GameState::new_two_player();
        state.step = Step::Cleanup;
        state.priority = state.active_player;
        // Nine cards in hand — over the seven-card maximum (CR 514.1), with room to
        // shed one and still be over the limit (used by the stale-token test).
        let hand: Vec<CardInstance> = (0..9).map(|_| state.new_instance(CardId(5))).collect();
        state.players[state.active_player.0].hand = hand.clone();
        (state, hand)
    }

    /// The cleanup discard-to-maximum (CR 514.1) collapses the engine's per-card
    /// `Discard` list into ONE `discard` action carrying a single `select_from_zone`
    /// prompt over the hand (issue #156, the flagship `select_from_zone` projection);
    /// a chosen card resolves to that concrete [`Action::Discard`].
    #[test]
    fn cleanup_discard_projects_select_from_zone_and_a_selection_resolves() {
        let db = CardDatabase::bundled().unwrap();
        let (state, hand) = cleanup_over_hand_limit();

        let view = personalized_view(&state, &db, state.active_player);

        // Exactly one `discard` action (the N per-card actions are gone), token-bound.
        let discards: Vec<&ValidAction> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "discard")
            .collect();
        assert_eq!(discards.len(), 1, "one collapsed discard, not one per card");
        let discard = discards[0];
        assert!(!discard.token.is_empty());

        // It carries one select-from-zone slot over the hand (count 1, all cards).
        assert_eq!(discard.prompts.len(), 1);
        let Prompt::SelectFromZone {
            slot,
            zone,
            owner,
            count,
            candidates,
            ..
        } = &discard.prompts[0]
        else {
            panic!("the discard is a select-from-zone prompt");
        };
        assert_eq!(slot, "discard");
        assert_eq!(zone, "hand");
        assert_eq!(owner, &player_id(state.active_player));
        assert_eq!(*count, 1);
        assert_eq!(
            *candidates,
            hand.iter()
                .map(|c| card_entity_id(c.id))
                .collect::<Vec<_>>(),
        );

        // Choosing one card resolves to a Discard of exactly that instance.
        let choose = ChooseAction {
            action_id: discard.id.clone(),
            token: discard.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec![card_entity_id(hand[3].id)],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, state.active_player, &choose),
            Some(Action::Discard { card: hand[3] }),
        );

        // A card not among the candidates (never in hand) is rejected.
        let foreign = ChooseAction {
            action_id: discard.id.clone(),
            token: discard.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec!["card_99999".to_string()],
            }],
        };
        assert!(resolve_action(&state, &db, state.active_player, &foreign).is_none());
    }

    /// Content binding (ADR 0009) covers the new prompt shapes too: a token captured
    /// for a `select_from_zone` discard while the hand is one shape is rejected once
    /// the hand — and so the prompt's candidates — has changed, exactly as it is for a
    /// targeted action. A stale prompt answer can never rebind.
    #[test]
    fn stale_token_on_a_prompt_action_is_rejected() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = cleanup_over_hand_limit();

        // Capture the answer a client would send for the discard action now.
        let before = personalized_view(&state, &db, state.active_player);
        let discard_before = before
            .valid_actions
            .iter()
            .find(|a| a.kind == "discard")
            .expect("a discard is offered while over the hand limit");
        let stale = ChooseAction {
            action_id: discard_before.id.clone(),
            token: discard_before.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec![card_entity_id(hand[0].id)],
            }],
        };

        // The hand changes (a card leaves), so the prompt's candidates — and thus the
        // action's content token — change under the same positional id.
        let seat = state.active_player.0;
        state.players[seat].hand.remove(1);
        let after = personalized_view(&state, &db, state.active_player);
        let discard_after = after
            .valid_actions
            .iter()
            .find(|a| a.id == stale.action_id)
            .expect("the id is still offered");
        assert_ne!(
            discard_before.token, discard_after.token,
            "changed candidates re-derive a different content token",
        );

        // The stale token no longer matches, so the answer is rejected.
        assert!(resolve_action(&state, &db, state.active_player, &stale).is_none());

        // The current token for that same id does resolve, proving it is the token
        // (not the bare id) that binds a prompt answer.
        let fresh = ChooseAction {
            action_id: discard_after.id.clone(),
            token: discard_after.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec![card_entity_id(hand[0].id)],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, state.active_player, &fresh),
            Some(Action::Discard { card: hand[0] }),
        );
    }

    /// The declare-attackers view advertises the engine's attacker candidates
    /// (CR 508.1a) as a multi-select `requirements` slot, and a returned selection
    /// resolves to a `DeclareAttackers` naming exactly those permanents (issue #140).
    #[test]
    fn issue_140_declare_attackers_projects_candidates_and_a_selection_resolves() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.step = Step::DeclareAttackers;
        // An eligible attacker (untapped, non-sick creature) for the active player,
        // plus a tapped one that is not a candidate.
        let attacker = put_permanent(&mut state, CardId(6), PlayerId(0), false, false);
        let _tapped = put_permanent(&mut state, CardId(6), PlayerId(0), true, false);

        let view = personalized_view(&state, &db, PlayerId(0));
        let declare = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "declare_attackers")
            .expect("the active player declares attackers");
        assert_eq!(declare.requirements.len(), 1);
        assert_eq!(declare.requirements[0].slot, "attackers");
        assert_eq!(
            declare.requirements[0].candidates,
            vec![permanent_entity_id(attacker)],
            "only the eligible attacker is a candidate",
        );

        let choose = ChooseAction {
            action_id: declare.id.clone(),
            token: declare.token.clone(),
            targets: vec![TargetChoice {
                slot: "attackers".to_string(),
                chosen: vec![permanent_entity_id(attacker)],
            }],
        };
        let resolved =
            resolve_action(&state, &db, PlayerId(0), &choose).expect("the selection resolves");
        assert_eq!(
            resolved,
            Action::DeclareAttackers {
                attackers: vec![attacker],
            },
        );

        // Declaring no attackers stays legal: the token-bound answer with an empty
        // selection resolves to an empty declaration (optional multi-select).
        let none = ChooseAction {
            action_id: declare.id.clone(),
            token: declare.token.clone(),
            targets: Vec::new(),
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &none),
            Some(Action::DeclareAttackers {
                attackers: Vec::new(),
            }),
        );
    }

    /// The declare-blockers view advertises one slot per declared attacker
    /// (CR 509.1a), each listing the defender's eligible blockers, and a returned
    /// blocker→attacker assignment resolves to a `DeclareBlockers` (issue #140).
    #[test]
    fn issue_140_declare_blockers_projects_candidates_and_a_selection_resolves() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.step = Step::DeclareBlockers;
        // The defending player (seat 1) is deciding.
        state.priority = PlayerId(1);
        let attacker = put_permanent(&mut state, CardId(6), PlayerId(0), true, true);
        let blocker = put_permanent(&mut state, CardId(6), PlayerId(1), false, false);

        let view = personalized_view(&state, &db, PlayerId(1));
        let declare = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "declare_blockers")
            .expect("the defender declares blockers");
        assert_eq!(
            declare.requirements.len(),
            1,
            "one slot per declared attacker"
        );
        assert_eq!(declare.requirements[0].slot, blocker_slot(attacker));
        assert_eq!(
            declare.requirements[0].candidates,
            vec![permanent_entity_id(blocker)],
        );

        let choose = ChooseAction {
            action_id: declare.id.clone(),
            token: declare.token.clone(),
            targets: vec![TargetChoice {
                slot: blocker_slot(attacker),
                chosen: vec![permanent_entity_id(blocker)],
            }],
        };
        let resolved =
            resolve_action(&state, &db, PlayerId(1), &choose).expect("the assignment resolves");
        assert_eq!(
            resolved,
            Action::DeclareBlockers {
                blocks: vec![Block { blocker, attacker }],
            },
        );
    }

    /// The ability-target `requirements` projection (ADR 0009 deferral #73, folded
    /// into issue #140): a `{T}: Tap target creature` activation advertises its one
    /// target slot with the legal creature candidates, and a returned target
    /// resolves to an `ActivateAbility` carrying exactly that chosen target.
    #[test]
    fn issue_140_ability_target_requirements_project_and_a_selection_resolves() {
        // A Tapper artifact ({T}: Tap target creature) and a Bear to target.
        let json = r#"[
            {"id":200,"name":"Tapper","types":["artifact"],"mana_cost":"","oracle_text":"",
             "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                          "effects":[{"kind":"tap","target":"any_creature"}]}]},
            {"id":201,"name":"Bear","types":["creature"],"mana_cost":"","oracle_text":"",
             "power":2,"toughness":2}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let tapper = put_permanent(&mut state, CardId(200), PlayerId(0), false, false);
        let bear = put_permanent(&mut state, CardId(201), PlayerId(0), false, false);

        let view = personalized_view(&state, &db, PlayerId(0));
        let activate = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "activate_ability")
            .expect("the Tapper's ability is activatable");
        assert_eq!(activate.subject, vec![permanent_entity_id(tapper)]);
        assert_eq!(activate.requirements.len(), 1, "one target slot");
        assert_eq!(activate.requirements[0].slot, "t0");
        assert_eq!(
            activate.requirements[0].candidates,
            vec![permanent_entity_id(bear)],
            "only the creature is a legal target (not the Tapper itself)",
        );

        let choose = ChooseAction {
            action_id: activate.id.clone(),
            token: activate.token.clone(),
            targets: vec![TargetChoice {
                slot: "t0".to_string(),
                chosen: vec![permanent_entity_id(bear)],
            }],
        };
        let resolved =
            resolve_action(&state, &db, PlayerId(0), &choose).expect("the target resolves");
        assert_eq!(
            resolved,
            Action::ActivateAbility {
                permanent: tapper,
                index: 0,
                targets: vec![Target::Permanent(bear)],
            },
        );

        // A target outside the advertised candidates (the Tapper itself) is rejected.
        let illegal = ChooseAction {
            action_id: activate.id.clone(),
            token: activate.token.clone(),
            targets: vec![TargetChoice {
                slot: "t0".to_string(),
                chosen: vec![permanent_entity_id(tapper)],
            }],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &illegal).is_none());
    }
}
