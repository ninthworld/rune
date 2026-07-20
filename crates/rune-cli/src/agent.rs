//! Non-interactive **agent mode** for the RUNE CLI (dev sequence step 4,
//! `docs/brief.md`: "AI opponents working").
//!
//! Agent mode reuses the exact same connection loop as the interactive client
//! (#32) — receive a personalized [`GameView`], reply with a `choose_action` —
//! but replaces the stdin prompt with a decision from an [`Agent`]. The agent is
//! any backend that, given the view, returns the `id` of one of the offered
//! `valid_actions`. A real deployment would hand the [`request_payload`] JSON to
//! an LLM and parse an id back; tests use a deterministic stub.
//!
//! The loop is the enforcement point for the `AGENTS.md` hard rule that the
//! client computes **no** game logic: the model only *picks among* actions the
//! engine already offered, and the choice is validated against that offered set
//! ([`is_offered`]) before it is sent. On any error, timeout, or unoffered id the
//! loop substitutes a [`safe_default`] (pass priority) and logs why, so a slow or
//! broken model can never stall the game.

use std::future::Future;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rune_protocol::{
    CardView, ChooseAction, ClientMessage, GameView, Permanent, Prompt, TargetChoice,
    TargetRequirement, ValidAction,
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::{ConfigError, LobbyConfig, SessionError, WsRead, WsWrite};

/// The `kind` string the server uses for the pass-priority action
/// (`rune-server`'s `view.rs`). The safe default prefers this action.
const PASS_PRIORITY_KIND: &str = "pass_priority";

/// Environment variable naming the agent decision deadline, in seconds. Overridden
/// by the `--agent-timeout` flag; ignored unless agent mode is enabled.
pub const AGENT_TIMEOUT_ENV_VAR: &str = "RUNE_AGENT_TIMEOUT";

/// Default deadline for a single agent decision when nothing overrides it.
pub const DEFAULT_AGENT_DEADLINE: Duration = Duration::from_secs(5);

/// A model backend that chooses one of the offered actions for a [`GameView`].
///
/// This is the seam that keeps a live model out of CI: the loop is generic over
/// `Agent`, so tests substitute a deterministic stub while a real provider (an
/// LLM over HTTP) implements the same method. Implementations perform **no** game
/// logic — they only select among the `valid_actions` the engine already offered,
/// and the caller re-validates the returned id regardless.
pub trait Agent {
    /// Choose the `id` of one entry of `view.valid_actions`.
    ///
    /// The returned id *should* be one the view offered, but the caller does not
    /// trust it: an [`Err`], a deadline overrun, or an id not in the offered set
    /// all fall back to [`safe_default`]. Return [`AgentError`] when the backend
    /// cannot produce a usable answer (network failure, unparseable response).
    fn choose(&self, view: &GameView) -> impl Future<Output = Result<String, AgentError>> + Send;
}

/// A backend failure that triggers the documented fallback.
#[derive(Debug)]
pub enum AgentError {
    /// The backend could not produce a usable decision — a network/provider
    /// error or an unparseable response. Carries a reason for the fallback log.
    Backend(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for AgentError {}

/// A minimal, deterministic, network-free agent: it passes priority when that
/// action is offered, otherwise takes the first offered action.
///
/// It is a ready-made stub for tests and the safe-fallback baseline; the binary's
/// actual `--agent` opponent is the [`RuleBasedAgent`], which plays to a win. It
/// never stalls: every actionable view yields exactly the [`safe_default`] choice.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassPriorityAgent;

impl Agent for PassPriorityAgent {
    async fn choose(&self, view: &GameView) -> Result<String, AgentError> {
        safe_default(view)
            .map(str::to_string)
            .ok_or_else(|| AgentError::Backend("no actions were offered".to_string()))
    }
}

// ---------------------------------------------------------------------------
// Rule-based agent (issue #159)
//
// A deterministic, network-free policy that plays a full, legal game to a win
// from `GameView` + `valid_actions` alone — never any engine access, no game logic
// beyond heuristics over the view. It answers **every** action shape the server
// emits: the collapsed `mulligan_decision` (option) and `discard`
// (select-from-zone) prompts, the combat `declare_attackers`/`declare_blockers`
// multi-select requirements, ability-target requirements, and the plain
// main-phase actions — falling back to a pass only when a pass is genuinely the
// only sound move. The companion [`fill_answers`] fills a chosen action's slots.
// ---------------------------------------------------------------------------

/// A deterministic, network-free **rule-based agent** that plays a full, legal
/// game to completion from a [`GameView`] alone (issue #159). It replaces the
/// pass-only [`PassPriorityAgent`] as the binary's opponent so two `--agent`
/// processes finish a real game.
///
/// Its priority policy, over whichever actions the server offers:
/// - **Mulligan** (option prompt): keep the opening hand rather than mulligan.
/// - **Cleanup discard** (select-from-zone prompt): discard the highest
///   mana-value card, keeping cheap, castable cards.
/// - **Combat**: declare attackers (every creature that can attack, since an
///   unblocked attacker only deals damage) and declare profitable blocks (a
///   blocker that survives or trades up), advancing combat every step.
/// - **Main phase**: play a land, cast the highest mana-value affordable creature,
///   then any other affordable spell, tapping lands for mana first to pay.
/// - Otherwise **pass priority**; never concede unless it is the only action.
///
/// Every choice breaks ties by the server's stable action/candidate order, so two
/// agents on a pinned `rng_seed` reproduce the same game. It never returns an id
/// the view did not offer, and never fills a slot with an un-advertised id
/// ([`fill_answers`]); the loop re-validates the chosen id regardless.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedAgent;

impl Agent for RuleBasedAgent {
    async fn choose(&self, view: &GameView) -> Result<String, AgentError> {
        choose_action(view)
            .map(|action| action.id.clone())
            .ok_or_else(|| AgentError::Backend("no actions were offered".to_string()))
    }
}

/// The action the [`RuleBasedAgent`] takes for `view`, or `None` when the view
/// offers nothing. Pure and deterministic — a function of the view alone, breaking
/// every tie by the server's stable action order (issue #159).
///
/// The server offers the special choices (mulligan, cleanup discard, combat
/// declarations) in their own windows with no `pass_priority` alongside, so
/// checking them before the main-phase development and the final pass yields a
/// single well-defined move for every view.
#[must_use]
pub fn choose_action(view: &GameView) -> Option<&ValidAction> {
    let actions = &view.valid_actions;
    if actions.is_empty() {
        return None;
    }

    // Pre-game mulligan (CR 103.5): keep the opening hand.
    if let Some(decision) = first_of_kind(actions, "mulligan_decision") {
        return Some(decision);
    }
    // Cleanup discard-to-max (CR 514.1): shed the costliest card.
    if let Some(discard) = first_of_kind(actions, "discard") {
        return Some(discard);
    }
    // Combat (CR 508/509): take the declaration to advance combat and apply
    // pressure. The selection itself is chosen in `fill_answers`.
    if let Some(attack) = first_of_kind(actions, "declare_attackers") {
        return Some(attack);
    }
    if let Some(block) = first_of_kind(actions, "declare_blockers") {
        return Some(block);
    }

    // Main-phase development: land, then the biggest affordable creature, then any
    // other affordable spell.
    if let Some(land) = first_of_kind(actions, "play_land") {
        return Some(land);
    }
    if let Some(creature) =
        highest_mana_value_where(view, actions, "cast_spell", subject_is_creature)
    {
        return Some(creature);
    }
    if let Some(spell) = highest_mana_value_where(view, actions, "cast_spell", |_, _| true) {
        return Some(spell);
    }
    // Build mana toward a spell: chain the offered tap-for-mana abilities.
    if wants_to_cast(view) {
        if let Some(mana) = actions.iter().find(|a| is_mana_source(view, a)) {
            return Some(mana);
        }
    }

    // Nothing to develop: pass priority when offered.
    if let Some(pass) = first_of_kind(actions, PASS_PRIORITY_KIND) {
        return Some(pass);
    }

    // No pass available (a forced choice): take any non-concede action, else concede
    // as the last resort so the game never stalls.
    actions
        .iter()
        .find(|a| a.kind != "concede")
        .or_else(|| actions.first())
}

/// The first offered action of `kind`, or `None`.
fn first_of_kind<'a>(actions: &'a [ValidAction], kind: &str) -> Option<&'a ValidAction> {
    actions.iter().find(|action| action.kind == kind)
}

/// The offered action of `kind` satisfying `pred` with the greatest mana value,
/// resolving ties toward the earliest in the server's stable order (deterministic).
fn highest_mana_value_where<'a>(
    view: &GameView,
    actions: &'a [ValidAction],
    kind: &str,
    pred: impl Fn(&GameView, &ValidAction) -> bool,
) -> Option<&'a ValidAction> {
    actions
        .iter()
        .filter(|action| action.kind == kind && pred(view, action))
        .fold(None, |best: Option<&ValidAction>, action| match best {
            // Keep the earliest action at the maximum value (replace only on a
            // strictly greater one), so the choice is order-stable.
            Some(current)
                if action_mana_value(view, current) >= action_mana_value(view, action) =>
            {
                Some(current)
            }
            _ => Some(action),
        })
}

/// The mana value of the hand card an action's subject names, or `0` if the subject
/// is not a known hand card. Used to rank `cast_spell`/`discard` actions.
fn action_mana_value(view: &GameView, action: &ValidAction) -> u32 {
    action
        .subject
        .iter()
        .filter_map(|id| card_in_hand(view, id))
        .filter_map(|card| card.mana_cost.as_deref())
        .map(mana_value_of)
        .max()
        .unwrap_or(0)
}

/// The converted mana value of a cost string like `"{2}{G}"` (→ 3): each `{N}`
/// generic pip adds `N`, every other symbol (colored, hybrid, …) adds 1. Empty or
/// unparsable segments contribute nothing. A pure lexical count — no color logic.
fn mana_value_of(cost: &str) -> u32 {
    cost.split('}')
        .filter_map(|segment| {
            let symbol = segment.trim_start_matches('{');
            if symbol.is_empty() {
                None
            } else {
                Some(symbol.parse::<u32>().unwrap_or(1))
            }
        })
        .sum()
}

/// The hand card `view` shows with entity id `id`, if the viewer holds it.
fn card_in_hand<'a>(view: &'a GameView, id: &str) -> Option<&'a CardView> {
    view.my_hand.iter().find(|card| card.id == id)
}

/// Whether a card is a creature, by its (server-computed) type line.
fn is_creature_card(card: &CardView) -> bool {
    card.type_line.to_lowercase().contains("creature")
}

/// Whether a card is a land, by its (server-computed) type line.
fn is_land_card(card: &CardView) -> bool {
    card.type_line.to_lowercase().contains("land")
}

/// Whether an action's subject names a creature card in hand (a creature spell).
fn subject_is_creature(view: &GameView, action: &ValidAction) -> bool {
    action
        .subject
        .iter()
        .filter_map(|id| card_in_hand(view, id))
        .any(is_creature_card)
}

/// Whether the hand holds any spell worth building mana for (any non-land card), so
/// the agent taps lands only when a cast is the goal — never pointlessly.
fn wants_to_cast(view: &GameView) -> bool {
    view.my_hand.iter().any(|card| !is_land_card(card))
}

/// The permanent `view` shows with entity id `id`, if any.
fn permanent_in_play<'a>(view: &'a GameView, id: &str) -> Option<&'a Permanent> {
    view.battlefield.iter().find(|perm| perm.id == id)
}

/// Whether an action is a land's mana ability — an `activate_ability` with no target
/// requirements whose source permanent is a land. The agent activates these to pay
/// for a spell; it leaves other activated abilities alone.
fn is_mana_source(view: &GameView, action: &ValidAction) -> bool {
    action.kind == "activate_ability"
        && action.requirements.is_empty()
        && action
            .subject
            .iter()
            .filter_map(|id| permanent_in_play(view, id))
            .any(|perm| is_land_card(&perm.card))
}

/// A permanent's power as a number, or `0` when absent/non-numeric (e.g. `"*"`).
fn power_of(perm: &Permanent) -> i64 {
    perm.card
        .power
        .as_deref()
        .and_then(|p| p.parse::<i64>().ok())
        .unwrap_or(0)
}

/// A permanent's toughness as a number, or `0` when absent/non-numeric.
fn toughness_of(perm: &Permanent) -> i64 {
    perm.card
        .toughness
        .as_deref()
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0)
}

/// Whether a permanent has printed keyword `keyword` (matched against the view's
/// wire keyword names, e.g. `"flying"`, `"reach"`).
fn has_keyword(perm: &Permanent, keyword: &str) -> bool {
    perm.card.keywords.iter().any(|k| k == keyword)
}

/// Whether `blocker` may legally be declared to block `attacker` given evasion
/// (CR 509.1b / 702.9c / 702.17b): a flying attacker can be blocked only by a
/// creature with flying or reach; any creature can block a non-flying attacker. The
/// engine does not pre-filter block candidates by evasion (it enforces it only when
/// the declaration is submitted), so the agent applies the same rule to avoid
/// picking — and endlessly resubmitting — an illegal block.
fn can_legally_block(attacker: Option<&Permanent>, blocker: &Permanent) -> bool {
    match attacker {
        Some(atk) if has_keyword(atk, "flying") => {
            has_keyword(blocker, "flying") || has_keyword(blocker, "reach")
        }
        _ => true,
    }
}

/// Fill every choice slot of `action` — its target `requirements` and its
/// [`Prompt`] slots — with a legal selection, one [`TargetChoice`] per slot, per the
/// [`RuleBasedAgent`] policy. `None` only when a **mandatory** slot cannot be filled
/// (an option with no options, or an ability-target slot with no candidates), which
/// the loop turns into a safe pass. A plain action (no slots) yields an empty
/// selection, so plain actions are unchanged.
///
/// Every chosen id is drawn from that slot's advertised candidates/options/items, so
/// the agent never submits an id the server did not enumerate (issue #159 property).
#[must_use]
pub fn fill_answers(view: &GameView, action: &ValidAction) -> Option<Vec<TargetChoice>> {
    let mut out: Vec<TargetChoice> = Vec::with_capacity(action.requirements.len());

    // Combat blocking assigns each blocker to at most one attacker, so the choice for
    // a `block_*` slot depends on which blockers earlier slots already used.
    let mut used_blockers: Vec<String> = Vec::new();

    for req in &action.requirements {
        let chosen: Vec<String> = if req.slot == "attackers" {
            attacker_selection(view, req)
        } else if req.slot.starts_with("defend_") {
            defender_selection(req)
        } else if req.slot.starts_with("block_") {
            block_selection(view, req, &mut used_blockers)
        } else if req.slot == "bottom" {
            // Mulligan bottoming (we never mulligan, so rarely hit): bottom the
            // required count of lowest mana-value cards.
            let count = leading_count(&req.prompt).unwrap_or(req.candidates.len());
            lowest_mana_value_ids(view, &req.candidates, count)
        } else {
            // An ability-target slot (`t0`, `t1`, …): a single mandatory target.
            vec![target_preference(view, req)?]
        };
        out.push(TargetChoice {
            slot: req.slot.clone(),
            chosen,
        });
    }

    for prompt in &action.prompts {
        let choice = match prompt {
            Prompt::Option { slot, options, .. } => {
                let picked = options
                    .iter()
                    .find(|option| {
                        option.id.eq_ignore_ascii_case("keep")
                            || option.label.to_lowercase().contains("keep")
                    })
                    .or_else(|| options.first())?;
                TargetChoice {
                    slot: slot.clone(),
                    chosen: vec![picked.id.clone()],
                }
            }
            Prompt::SelectFromZone {
                slot,
                prompt,
                count,
                candidates,
                ..
            } => {
                let n = *count as usize;
                // Discarding sheds the costliest cards; any other select-from-zone
                // (e.g. bottoming) keeps the cheap cards by shedding the cheapest.
                let ids = if prompt.to_lowercase().contains("discard") {
                    highest_mana_value_ids(view, candidates, n)
                } else {
                    lowest_mana_value_ids(view, candidates, n)
                };
                TargetChoice {
                    slot: slot.clone(),
                    chosen: ids,
                }
            }
            Prompt::Order { slot, items, .. } => TargetChoice {
                slot: slot.clone(),
                // "As given": echo the items in their advertised order.
                chosen: items.clone(),
            },
        };
        out.push(choice);
    }

    Some(out)
}

/// The attackers to declare: every candidate that can deal damage (power ≥ 1). An
/// unblocked attacker only ever deals damage, so attacking with the whole board is
/// the simplest sound aggressive rule; 0-power creatures are left back.
fn attacker_selection(view: &GameView, req: &TargetRequirement) -> Vec<String> {
    req.candidates
        .iter()
        .filter(|id| permanent_in_play(view, id).is_none_or(|perm| power_of(perm) >= 1))
        .cloned()
        .collect()
}

/// The defending player an attacker attacks, for a multiplayer `defend_<id>` slot
/// (issue #341/#345): the **first** advertised defender candidate. The server offers
/// these slots only when the active player has more than one opponent (a two-player
/// game has a sole defender and no slot); the engine lists the candidates in stable
/// seat order, so "first candidate" is a deterministic, always-legal choice — the
/// same simple, sound policy the agent uses elsewhere. `defend_*` slots for creatures
/// the agent does not attack with are ignored by the server, so answering every one is
/// harmless. Empty only if the slot somehow carries no candidate (then the attacker is
/// simply left undeclared).
fn defender_selection(req: &TargetRequirement) -> Vec<String> {
    req.candidates.first().cloned().into_iter().collect()
}

/// The blockers to assign to one attacker's `block_*` slot: at most one *profitable*
/// blocker (one that survives the block or trades up), chosen from the candidates not
/// already assigned to an earlier attacker. `GameView` carries no self-life, so the
/// agent cannot detect lethal to justify a chump; it therefore blocks only when the
/// block is favorable and never throws a creature away for nothing.
fn block_selection(
    view: &GameView,
    req: &TargetRequirement,
    used_blockers: &mut Vec<String>,
) -> Vec<String> {
    // The attacker this slot defends against (`block_<permId>`), for the P/T compare.
    let attacker = req
        .slot
        .strip_prefix("block_")
        .map(|suffix| format!("perm_{suffix}"))
        .and_then(|id| permanent_in_play(view, &id).cloned());
    let (attacker_power, attacker_toughness) = attacker
        .as_ref()
        .map_or((0, 0), |perm| (power_of(perm), toughness_of(perm)));

    let pick = req
        .candidates
        .iter()
        .filter(|id| !used_blockers.contains(*id))
        .find_map(|id| {
            let perm = permanent_in_play(view, id)?;
            // Evasion (CR 509.1b): a flyer can be blocked only by flying or reach. The
            // engine offers every untapped creature as a candidate and enforces
            // evasion only when the declaration is submitted (combat.rs), so the agent
            // must not pick an illegal blocker — resubmitting one would stall combat.
            if !can_legally_block(attacker.as_ref(), perm) {
                return None;
            }
            let survives = toughness_of(perm) > attacker_power;
            let kills = power_of(perm) >= attacker_toughness && attacker_toughness > 0;
            (survives || kills).then(|| id.clone())
        });

    match pick {
        Some(id) => {
            used_blockers.push(id.clone());
            vec![id]
        }
        None => Vec::new(),
    }
}

/// The preferred single target for an ability slot: an opponent (player or their
/// permanent) when the slot targets damage-style, otherwise the first advertised
/// candidate. `None` when the slot has no candidates (a mandatory slot the agent then
/// cannot answer).
fn target_preference(view: &GameView, req: &TargetRequirement) -> Option<String> {
    if req.candidates.is_empty() {
        return None;
    }
    // Prefer aiming at an opponent (their player id or a permanent they control),
    // which is the right default for the damage/tap abilities the engine models.
    let opponent_target = req
        .candidates
        .iter()
        .find(|id| is_opponent_target(view, id));
    opponent_target.or_else(|| req.candidates.first()).cloned()
}

/// Whether an entity id names an opponent — an opposing player, or a permanent an
/// opponent controls.
fn is_opponent_target(view: &GameView, id: &str) -> bool {
    view.opponents.iter().any(|opp| opp.player_id == id)
        || permanent_in_play(view, id).is_some_and(|perm| perm.controller != view.you)
}

/// The `count` candidate ids with the greatest hand mana value (ties broken toward
/// the advertised order), for shedding the costliest cards on a discard.
fn highest_mana_value_ids(view: &GameView, candidates: &[String], count: usize) -> Vec<String> {
    sorted_by_mana_value(view, candidates, true)
        .into_iter()
        .take(count)
        .collect()
}

/// The `count` candidate ids with the least hand mana value (ties broken toward the
/// advertised order), for bottoming/keeping the cheap cards.
fn lowest_mana_value_ids(view: &GameView, candidates: &[String], count: usize) -> Vec<String> {
    sorted_by_mana_value(view, candidates, false)
        .into_iter()
        .take(count)
        .collect()
}

/// `candidates` ordered by the mana value of the hand card each names — descending
/// when `descending`, ascending otherwise. A **stable** sort, so equal-value ids keep
/// their advertised order and the result is deterministic.
fn sorted_by_mana_value(view: &GameView, candidates: &[String], descending: bool) -> Vec<String> {
    let mut ids: Vec<String> = candidates.to_vec();
    ids.sort_by_key(|id| {
        let value = card_in_hand(view, id)
            .and_then(|card| card.mana_cost.as_deref())
            .map_or(0, mana_value_of);
        if descending {
            // Negate for a descending order under an ascending stable sort.
            -(i64::from(value))
        } else {
            i64::from(value)
        }
    });
    ids
}

/// The leading integer of a prompt like `"Put 2 card(s) on the bottom …"` (→ `2`),
/// or `None` if the prompt opens with no number. Lets the agent honor a bottoming
/// count the wire requirement does not carry as a field.
fn leading_count(prompt: &str) -> Option<usize> {
    prompt
        .split_whitespace()
        .find_map(|word| word.parse::<usize>().ok())
}

/// Configuration for agent mode, parsed from CLI flags and the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    /// Whether the `--agent` flag selected non-interactive agent mode.
    pub enabled: bool,
    /// Maximum time to wait for a single agent decision before falling back.
    pub deadline: Duration,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            deadline: DEFAULT_AGENT_DEADLINE,
        }
    }
}

impl AgentConfig {
    /// Build an [`AgentConfig`] from process arguments and environment.
    ///
    /// `--agent` enables the mode; the deadline comes from `--agent-timeout
    /// <seconds>` (or `--agent-timeout=<seconds>`), else [`AGENT_TIMEOUT_ENV_VAR`],
    /// else [`DEFAULT_AGENT_DEADLINE`].
    ///
    /// # Errors
    /// Returns [`ConfigError`] if `--agent-timeout` is given without a value or
    /// with a value that is not a positive number of seconds.
    pub fn from_env_and_args() -> Result<Self, ConfigError> {
        Self::resolve(std::env::args().skip(1), |key| std::env::var(key).ok())
    }

    /// Core of [`AgentConfig::from_env_and_args`], with arguments and environment
    /// injected so it can be unit-tested without touching process globals.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if `--agent-timeout` is missing its value or the
    /// supplied timeout is not a positive, finite number of seconds.
    pub fn resolve<A, E>(args: A, env: E) -> Result<Self, ConfigError>
    where
        A: IntoIterator<Item = String>,
        E: Fn(&str) -> Option<String>,
    {
        let mut enabled = false;
        let mut timeout: Option<String> = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--agent" {
                enabled = true;
            } else if let Some(value) = arg.strip_prefix("--agent-timeout=") {
                timeout = Some(value.to_string());
            } else if arg == "--agent-timeout" {
                timeout = Some(args.next().ok_or(ConfigError::MissingAgentTimeoutValue)?);
            }
        }

        let deadline = match timeout.or_else(|| env(AGENT_TIMEOUT_ENV_VAR)) {
            Some(raw) => parse_deadline(&raw)?,
            None => DEFAULT_AGENT_DEADLINE,
        };
        Ok(Self { enabled, deadline })
    }
}

/// Parse a positive, finite number of seconds into a [`Duration`].
fn parse_deadline(raw: &str) -> Result<Duration, ConfigError> {
    let seconds: f64 = raw
        .trim()
        .parse()
        .map_err(|_| ConfigError::InvalidAgentTimeout(raw.to_string()))?;
    if seconds.is_finite() && seconds > 0.0 {
        Ok(Duration::from_secs_f64(seconds))
    } else {
        Err(ConfigError::InvalidAgentTimeout(raw.to_string()))
    }
}

/// Serialize the exact JSON a model backend should be given: the personalized
/// [`GameView`], which already carries the offered `valid_actions` and only what
/// the receiving player is entitled to see. A real provider embeds this string in
/// its prompt; nothing else about the session (server address, credentials) is
/// exposed to the model.
///
/// # Errors
/// Returns the underlying [`serde_json::Error`] if the view cannot be serialized.
pub fn request_payload(view: &GameView) -> Result<String, serde_json::Error> {
    serde_json::to_string(view)
}

/// Whether `id` names one of the actions `view` offered. This is the client's
/// only check on the model's answer — it never computes legality, only membership.
#[must_use]
pub fn is_offered(view: &GameView, id: &str) -> bool {
    view.valid_actions.iter().any(|action| action.id == id)
}

/// The safe fallback choice for `view`: the pass-priority action if offered,
/// otherwise the first offered action, otherwise `None` (no actions at all).
///
/// Passing priority is always legal when the player holds it, so this never
/// stalls the game when substituted for a failed model decision.
#[must_use]
pub fn safe_default(view: &GameView) -> Option<&str> {
    view.valid_actions
        .iter()
        .find(|action| action.kind == PASS_PRIORITY_KIND)
        .or_else(|| view.valid_actions.first())
        .map(|action| action.id.as_str())
}

/// The label of the offered action with `id`, for logging; a placeholder if the
/// id is not among the offered actions.
fn label_for(view: &GameView, id: &str) -> String {
    view.valid_actions
        .iter()
        .find(|action| action.id == id)
        .map_or_else(
            || "unknown action".to_string(),
            |action| action.label.clone(),
        )
}

/// Run agent mode to completion over an already-connected socket.
///
/// The loop mirrors the interactive [`run_session`](crate::run_session): receive
/// one [`GameView`], and — only when it offers actions — ask `agent` to choose,
/// validate the choice, and send the matching `action_id` as a
/// [`ClientMessage::ChooseAction`]. A view with no actions is skipped. Every
/// decision is raced against `deadline`; a timeout, backend error, or unoffered
/// id logs the reason to `log` and sends the [`safe_default`] instead.
///
/// `log` receives human-readable, one-line decision notes (stderr in the binary,
/// an in-memory buffer in tests). The loop exits cleanly — `Ok(())` — when the
/// server closes the connection, and returns an error only if the transport or a
/// local encode/log write fails mid-session.
///
/// # Errors
/// Returns a [`SessionError`] if a WebSocket read/write, the encoding of a chosen
/// action, or a write to `log` fails.
pub async fn run_agent_session<S, W, A>(
    ws: WebSocketStream<S>,
    agent: &A,
    deadline: Duration,
    mut log: W,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    W: AsyncWrite + Unpin,
    A: Agent,
{
    let (mut write, mut read) = ws.split();
    agent_game_loop(&mut write, &mut read, agent, deadline, &mut log, None).await
}

/// Run the full unattended flow over an already-connected socket: drive the lobby
/// from `plan` (create/join a room, submit a deck, ready), then play the game with
/// `agent` once the ready gate passes (ADR 0012, issue #115).
///
/// The lobby phase sends only commands the server offered and holds no game logic;
/// the instant the game is constructed the server pushes the first `GameView` on the
/// same socket and this hands off to [`agent_game_loop`]. `plan` should name a room
/// action (`--create`/`--room`) and a `--deck`, or the agent has nothing to do and
/// waits until the server closes.
///
/// # Errors
/// Returns a [`SessionError`] if a WebSocket read/write, the encoding of a
/// command/action, or a write to `log` fails.
pub async fn run_agent_lobby_session<S, W, A>(
    ws: WebSocketStream<S>,
    agent: &A,
    deadline: Duration,
    mut log: W,
    plan: &LobbyConfig,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    W: AsyncWrite + Unpin,
    A: Agent,
{
    let (mut write, mut read) = ws.split();
    match crate::lobby::run_lobby_agent(&mut write, &mut read, &mut log, plan).await? {
        Some(first_view) => {
            agent_game_loop(
                &mut write,
                &mut read,
                agent,
                deadline,
                &mut log,
                Some(first_view),
            )
            .await
        }
        None => {
            let _ = write.close().await;
            Ok(())
        }
    }
}

/// The in-game agent loop over a split socket: receive a `GameView`, and — when it
/// offers actions — ask `agent` to choose, then send the chosen action id, its
/// content-binding `token` (echoed verbatim), and any targets (ADR 0009).
/// `first_view` lets the lobby hand off the first game frame it already read.
pub(crate) async fn agent_game_loop<S, W, A>(
    write: &mut WsWrite<S>,
    read: &mut WsRead<S>,
    agent: &A,
    deadline: Duration,
    log: &mut W,
    first_view: Option<GameView>,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    W: AsyncWrite + Unpin,
    A: Agent,
{
    let mut pending = first_view;

    'session: loop {
        // 1. Receive the next personalized view — the whole decision context is
        //    rebuilt from this one message; nothing is carried across frames.
        let view = match pending.take() {
            Some(view) => view,
            None => match next_agent_view(read, log).await? {
                Some(view) => view,
                None => break 'session,
            },
        };

        // 2. No actions offered (we do not hold priority): await the next view.
        if view.valid_actions.is_empty() {
            continue;
        }

        // 3. Ask the agent, with a hard deadline and a validated, safe fallback, then
        //    build the answer echoing the action's content-binding token.
        if let Some(action_id) = decide(agent, &view, deadline, log).await? {
            if let Some(choose) = agent_choice(&view, &action_id, log).await? {
                let message = ClientMessage::ChooseAction(choose);
                let json = serde_json::to_string(&message).map_err(SessionError::Encode)?;
                write
                    .send(Message::Text(json))
                    .await
                    .map_err(SessionError::WebSocket)?;
            }
        }
    }

    let _ = write.send(Message::Close(None)).await;
    Ok(())
}

/// Read frames until the next decodable [`GameView`] arrives, returning `None` when
/// the server closes the connection. Undecodable text frames are logged and skipped.
async fn next_agent_view<S, W>(
    read: &mut WsRead<S>,
    log: &mut W,
) -> Result<Option<GameView>, SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    W: AsyncWrite + Unpin,
{
    loop {
        match read.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<GameView>(text.as_str()) {
                    Ok(view) => return Ok(Some(view)),
                    Err(error) => {
                        let note = format!("agent: ignoring undecodable server message: {error}\n");
                        log_line(log, &note).await?;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => {
                log_line(log, "agent: server closed the connection.\n").await?;
                return Ok(None);
            }
            // Ping/pong/binary/raw frames carry no protocol message; ignore.
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(SessionError::WebSocket(error)),
        }
    }
}

/// Build the [`ChooseAction`] to send for a chosen action id: echo the offered
/// action's content-binding `token` verbatim (ADR 0009) and fill every one of its
/// target `requirements` and [`Prompt`] slots via [`fill_answers`] (issue #159), so
/// the mulligan, cleanup discard, combat declarations, and ability targets are all
/// answered rather than passed over. If a **mandatory** slot cannot be filled (an
/// ability target with no candidates), this falls back to a requirement-less pass
/// rather than send an answer the server would reject and re-offer forever.
async fn agent_choice<W>(
    view: &GameView,
    action_id: &str,
    log: &mut W,
) -> Result<Option<ChooseAction>, SessionError>
where
    W: AsyncWrite + Unpin,
{
    let Some(action) = view.valid_actions.iter().find(|a| a.id == action_id) else {
        return Ok(None);
    };
    if let Some(targets) = fill_answers(view, action) {
        return Ok(Some(ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets,
        }));
    }

    // A mandatory slot has no legal candidate; substitute a requirement-less pass so
    // the game never stalls on a rejected answer.
    match view
        .valid_actions
        .iter()
        .find(|a| a.kind == PASS_PRIORITY_KIND && a.requirements.is_empty() && a.prompts.is_empty())
    {
        Some(pass) => {
            let note = format!(
                "agent: {:?} cannot fill a required slot — passing instead\n",
                action.id
            );
            log_line(log, &note).await?;
            Ok(Some(ChooseAction {
                action_id: pass.id.clone(),
                token: pass.token.clone(),
                targets: Vec::new(),
            }))
        }
        None => {
            log_line(
                log,
                "agent: chosen action cannot be answered and no pass is available — skipping\n",
            )
            .await?;
            Ok(None)
        }
    }
}

/// Resolve one actionable view to the `action_id` to send, applying the deadline
/// and fallback. Returns `None` only in the impossible case of an actionable view
/// with no offered actions, so the caller simply sends nothing and waits.
async fn decide<A, W>(
    agent: &A,
    view: &GameView,
    deadline: Duration,
    log: &mut W,
) -> Result<Option<String>, SessionError>
where
    A: Agent,
    W: AsyncWrite + Unpin,
{
    match tokio::time::timeout(deadline, agent.choose(view)).await {
        Ok(Ok(id)) if is_offered(view, &id) => {
            let note = format!("agent: chose {id:?} ({})\n", label_for(view, &id));
            log_line(log, &note).await?;
            Ok(Some(id))
        }
        Ok(Ok(id)) => fall_back(view, log, &format!("model returned unoffered id {id:?}")).await,
        Ok(Err(error)) => fall_back(view, log, &format!("backend error: {error}")).await,
        Err(_elapsed) => fall_back(view, log, &format!("model timed out after {deadline:?}")).await,
    }
}

/// Log `reason` and resolve to the [`safe_default`] action for `view`.
async fn fall_back<W>(
    view: &GameView,
    log: &mut W,
    reason: &str,
) -> Result<Option<String>, SessionError>
where
    W: AsyncWrite + Unpin,
{
    match safe_default(view) {
        Some(id) => {
            let note = format!(
                "agent: fell back to {id:?} ({}) — {reason}\n",
                label_for(view, id)
            );
            log_line(log, &note).await?;
            Ok(Some(id.to_string()))
        }
        None => {
            let note = format!("agent: no action to take — {reason}\n");
            log_line(log, &note).await?;
            Ok(None)
        }
    }
}

/// Write one log line and flush it, mapping any I/O failure to [`SessionError`].
async fn log_line<W: AsyncWrite + Unpin>(log: &mut W, text: &str) -> Result<(), SessionError> {
    log.write_all(text.as_bytes())
        .await
        .map_err(SessionError::Io)?;
    log.flush().await.map_err(SessionError::Io)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use rune_protocol::{
        CardView, OpponentView, Permanent, Phase, PromptOption, TargetRequirement, ValidAction,
    };

    fn view_with_actions(actions: Vec<ValidAction>) -> GameView {
        GameView {
            you: "p0".into(),
            my_hand: vec![],
            me: rune_protocol::SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::PrecombatMain,
            turn: 1,
            active_player: "p0".into(),
            mana_pool: vec![],
            priority_player: Some("p0".into()),
            valid_actions: actions,
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: std::collections::BTreeMap::new(),
            commander_damage: Vec::new(),
            seat_order: Vec::new(),
        }
    }

    fn pass() -> ValidAction {
        ValidAction {
            id: "a0".into(),
            kind: "pass_priority".into(),
            label: "Pass priority".into(),
            subject: vec![],
            ..Default::default()
        }
    }

    fn play_land() -> ValidAction {
        ValidAction {
            id: "a1".into(),
            kind: "play_land".into(),
            label: "Play Forest".into(),
            subject: vec!["card_5".into()],
            ..Default::default()
        }
    }

    /// Stub that always returns a fixed id, offered or not.
    struct AlwaysChoose(&'static str);
    impl Agent for AlwaysChoose {
        async fn choose(&self, _view: &GameView) -> Result<String, AgentError> {
            Ok(self.0.to_string())
        }
    }

    /// Stub whose backend always fails.
    struct AlwaysError;
    impl Agent for AlwaysError {
        async fn choose(&self, _view: &GameView) -> Result<String, AgentError> {
            Err(AgentError::Backend("boom".to_string()))
        }
    }

    /// Stub that answers only after `delay`, to exercise the deadline.
    struct SlowAgent {
        id: &'static str,
        delay: Duration,
    }
    impl Agent for SlowAgent {
        async fn choose(&self, _view: &GameView) -> Result<String, AgentError> {
            tokio::time::sleep(self.delay).await;
            Ok(self.id.to_string())
        }
    }

    #[tokio::test]
    async fn valid_choice_is_sent_verbatim() {
        let view = view_with_actions(vec![pass(), play_land()]);
        let mut log = Vec::new();
        let chosen = decide(&AlwaysChoose("a1"), &view, Duration::from_secs(1), &mut log)
            .await
            .unwrap();
        assert_eq!(chosen.as_deref(), Some("a1"));
        let text = String::from_utf8(log).unwrap();
        assert!(text.contains("chose"), "logs the decision:\n{text}");
        assert!(
            !text.contains("fell back"),
            "no fallback for a valid id:\n{text}"
        );
    }

    #[tokio::test]
    async fn out_of_set_choice_falls_back_to_pass() {
        // pass is offered second; the fallback must find it by kind, not position.
        let view = view_with_actions(vec![play_land(), pass()]);
        let mut log = Vec::new();
        let chosen = decide(
            &AlwaysChoose("does_not_exist"),
            &view,
            Duration::from_secs(1),
            &mut log,
        )
        .await
        .unwrap();
        assert_eq!(
            chosen.as_deref(),
            Some("a0"),
            "fell back to the pass action"
        );
        let text = String::from_utf8(log).unwrap();
        assert!(text.contains("fell back"), "logs the fallback:\n{text}");
        assert!(text.contains("unoffered"), "logs why:\n{text}");
    }

    #[tokio::test]
    async fn backend_error_falls_back_and_logs_reason() {
        let view = view_with_actions(vec![pass(), play_land()]);
        let mut log = Vec::new();
        let chosen = decide(&AlwaysError, &view, Duration::from_secs(1), &mut log)
            .await
            .unwrap();
        assert_eq!(chosen.as_deref(), Some("a0"));
        assert!(String::from_utf8(log).unwrap().contains("backend error"));
    }

    #[tokio::test]
    async fn slow_agent_times_out_and_falls_back() {
        let view = view_with_actions(vec![pass(), play_land()]);
        let mut log = Vec::new();
        let chosen = decide(
            &SlowAgent {
                id: "a1",
                delay: Duration::from_millis(500),
            },
            &view,
            Duration::from_millis(10),
            &mut log,
        )
        .await
        .unwrap();
        assert_eq!(
            chosen.as_deref(),
            Some("a0"),
            "deadline forced the safe default"
        );
        assert!(String::from_utf8(log).unwrap().contains("timed out"));
    }

    #[test]
    fn request_payload_carries_view_and_actions_and_nothing_else() {
        let view = view_with_actions(vec![pass(), play_land()]);
        let payload = request_payload(&view).unwrap();
        // The model gets the offered actions and public state...
        assert!(payload.contains("valid_actions"), "payload: {payload}");
        assert!(payload.contains("\"a0\"") && payload.contains("\"a1\""));
        assert!(payload.contains("phase"));
        // ...and nothing beyond the GameView: it round-trips back to the same view.
        let back: GameView = serde_json::from_str(&payload).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn safe_default_prefers_pass_then_first_then_none() {
        assert_eq!(
            safe_default(&view_with_actions(vec![play_land(), pass()])),
            Some("a0")
        );
        assert_eq!(
            safe_default(&view_with_actions(vec![play_land()])),
            Some("a1")
        );
        assert_eq!(safe_default(&view_with_actions(vec![])), None);
    }

    #[test]
    fn is_offered_checks_membership_only() {
        let view = view_with_actions(vec![pass(), play_land()]);
        assert!(is_offered(&view, "a0"));
        assert!(!is_offered(&view, "a9"));
    }

    #[tokio::test]
    async fn pass_priority_agent_takes_the_offered_pass() {
        let view = view_with_actions(vec![play_land(), pass()]);
        let id = PassPriorityAgent.choose(&view).await.unwrap();
        assert_eq!(id, "a0");
    }

    // --- Rule-based agent (issue #159) ------------------------------------

    /// A hand card view with a mana cost and type line.
    fn hand_card(id: &str, type_line: &str, cost: &str) -> CardView {
        CardView {
            id: id.into(),
            name: id.into(),
            type_line: type_line.into(),
            mana_cost: (!cost.is_empty()).then(|| cost.to_string()),
            rules_text: String::new(),
            functional_id: String::new(),
            power: None,
            toughness: None,
            keywords: vec![],
        }
    }

    /// A battlefield creature permanent with the given controller and P/T.
    fn creature_perm(id: &str, controller: &str, power: i64, toughness: i64) -> Permanent {
        Permanent {
            id: id.into(),
            controller: controller.into(),
            owner: controller.into(),
            card: CardView {
                id: id.into(),
                name: id.into(),
                type_line: "Creature".into(),
                mana_cost: None,
                rules_text: String::new(),
                functional_id: String::new(),
                power: Some(power.to_string()),
                toughness: Some(toughness.to_string()),
                keywords: vec![],
            },
            tapped: false,
            attacking: false,
            attacking_player: None,
            blocking: None,
            damage: 0,
            attached_to: None,
            counters: vec![],
        }
    }

    fn action(id: &str, kind: &str, subject: Vec<&str>) -> ValidAction {
        ValidAction {
            id: id.into(),
            kind: kind.into(),
            label: kind.into(),
            subject: subject.into_iter().map(str::to_string).collect(),
            token: format!("tok_{id}"),
            ..Default::default()
        }
    }

    fn chosen_for<'a>(targets: &'a [TargetChoice], slot: &str) -> &'a [String] {
        targets
            .iter()
            .find(|t| t.slot == slot)
            .map_or(&[], |t| t.chosen.as_slice())
    }

    #[test]
    fn mana_value_of_sums_generic_and_symbols() {
        assert_eq!(mana_value_of(""), 0);
        assert_eq!(mana_value_of("{G}"), 1);
        assert_eq!(mana_value_of("{2}{G}"), 3);
        assert_eq!(mana_value_of("{4}{G}"), 5);
        assert_eq!(mana_value_of("{1}"), 1);
    }

    #[test]
    fn policy_keeps_at_the_mulligan_decision() {
        let decision = ValidAction {
            prompts: vec![Prompt::Option {
                slot: "decision".into(),
                prompt: "Keep this hand or take a mulligan?".into(),
                options: vec![
                    PromptOption {
                        id: "keep".into(),
                        label: "Keep this hand".into(),
                    },
                    PromptOption {
                        id: "mulligan".into(),
                        label: "Mulligan".into(),
                    },
                ],
            }],
            ..action("a0", "mulligan_decision", vec![])
        };
        let view = view_with_actions(vec![decision.clone(), action("a1", "concede", vec![])]);

        let picked = choose_action(&view).unwrap();
        assert_eq!(picked.kind, "mulligan_decision");
        let answers = fill_answers(&view, picked).unwrap();
        assert_eq!(chosen_for(&answers, "decision"), ["keep"]);
    }

    #[test]
    fn policy_discards_the_highest_mana_value_card() {
        // Over-full hand: a Forest (0), a 1-drop, and a 5-drop; discard the 5-drop.
        let mut view = view_with_actions(vec![
            ValidAction {
                prompts: vec![Prompt::SelectFromZone {
                    slot: "discard".into(),
                    prompt: "Choose a card to discard".into(),
                    zone: "hand".into(),
                    owner: "p0".into(),
                    count: 1,
                    candidates: vec!["card_forest".into(), "card_1".into(), "card_5".into()],
                }],
                ..action("a0", "discard", vec![])
            },
            action("a1", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_forest", "Land", ""),
            hand_card("card_1", "Creature", "{G}"),
            hand_card("card_5", "Creature", "{4}{G}"),
        ];

        let picked = choose_action(&view).unwrap();
        assert_eq!(picked.kind, "discard");
        let answers = fill_answers(&view, picked).unwrap();
        assert_eq!(chosen_for(&answers, "discard"), ["card_5"]);
    }

    #[test]
    fn policy_plays_a_land_before_casting_or_passing() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "play_land", vec!["card_forest"]),
            action("a2", "cast_spell", vec!["card_g"]),
            action("a3", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_forest", "Land", ""),
            hand_card("card_g", "Creature", "{G}"),
        ];
        assert_eq!(choose_action(&view).unwrap().kind, "play_land");
    }

    #[test]
    fn policy_casts_the_highest_mana_value_creature() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "cast_spell", vec!["card_g"]),
            action("a2", "cast_spell", vec!["card_5"]),
            action("a3", "cast_spell", vec!["card_art"]),
            action("a4", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_g", "Creature", "{G}"),
            hand_card("card_5", "Creature", "{4}{G}"),
            hand_card("card_art", "Artifact", "{1}"),
        ];
        // The 5-drop creature outranks the 1-drop creature and the artifact.
        let picked = choose_action(&view).unwrap();
        assert_eq!(picked.subject, vec!["card_5".to_string()]);
    }

    #[test]
    fn policy_casts_a_noncreature_when_no_creature_is_affordable() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "cast_spell", vec!["card_art"]),
            action("a2", "concede", vec![]),
        ]);
        view.my_hand = vec![hand_card("card_art", "Artifact", "{1}")];
        assert_eq!(choose_action(&view).unwrap().kind, "cast_spell");
    }

    #[test]
    fn policy_taps_a_land_for_mana_when_a_creature_is_uncast() {
        // No cast is offered yet (no mana), but a creature is in hand and a Forest
        // can be tapped: activate the mana ability to build toward the cast.
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "activate_ability", vec!["perm_forest"]),
            action("a2", "concede", vec![]),
        ]);
        view.my_hand = vec![hand_card("card_g", "Creature", "{G}")];
        view.battlefield = vec![Permanent {
            card: CardView {
                type_line: "Land".into(),
                ..creature_perm("perm_forest", "p0", 0, 0).card
            },
            ..creature_perm("perm_forest", "p0", 0, 0)
        }];
        assert_eq!(choose_action(&view).unwrap().kind, "activate_ability");
    }

    #[test]
    fn policy_does_not_tap_mana_with_an_empty_hand() {
        // A land ability is offered but there is nothing to cast: pass instead of
        // tapping pointlessly.
        let view = view_with_actions(vec![
            pass(),
            action("a1", "activate_ability", vec!["perm_forest"]),
            action("a2", "concede", vec![]),
        ]);
        assert_eq!(choose_action(&view).unwrap().kind, "pass_priority");
    }

    #[test]
    fn policy_passes_when_only_pass_and_concede_are_offered() {
        let view = view_with_actions(vec![pass(), action("a1", "concede", vec![])]);
        assert_eq!(choose_action(&view).unwrap().kind, "pass_priority");
    }

    #[test]
    fn policy_never_concedes_while_another_action_exists() {
        // A forced window offering only a declaration and concede: take the
        // declaration, never concede.
        let view = view_with_actions(vec![
            action("a0", "declare_attackers", vec![]),
            action("a1", "concede", vec![]),
        ]);
        assert_eq!(choose_action(&view).unwrap().kind, "declare_attackers");
    }

    #[test]
    fn attackers_selection_declares_every_damaging_creature() {
        let mut view = view_with_actions(vec![ValidAction {
            requirements: vec![TargetRequirement {
                slot: "attackers".into(),
                prompt: "Choose which creatures attack".into(),
                candidates: vec!["perm_a".into(), "perm_b".into(), "perm_wall".into()],
            }],
            ..action("a0", "declare_attackers", vec![])
        }]);
        view.you = "p0".into();
        view.battlefield = vec![
            creature_perm("perm_a", "p0", 2, 2),
            creature_perm("perm_b", "p0", 3, 3),
            creature_perm("perm_wall", "p0", 0, 4), // 0 power: held back
        ];
        let picked = choose_action(&view).unwrap();
        let answers = fill_answers(&view, picked).unwrap();
        let attackers = chosen_for(&answers, "attackers");
        assert!(attackers.contains(&"perm_a".to_string()));
        assert!(attackers.contains(&"perm_b".to_string()));
        assert!(!attackers.contains(&"perm_wall".to_string()));
    }

    #[test]
    fn blockers_selection_blocks_only_profitably_and_never_double_assigns() {
        // Two attackers; our two blockers: one survives/trades, one only chumps.
        let attacker_big = "perm_atk_big"; // 3/3
        let attacker_small = "perm_atk_small"; // 2/2
        let mut view = view_with_actions(vec![ValidAction {
            requirements: vec![
                TargetRequirement {
                    slot: format!("block_{}", "atk_big"),
                    prompt: "Choose blockers for Big".into(),
                    candidates: vec!["perm_blk_good".into(), "perm_blk_weak".into()],
                },
                TargetRequirement {
                    slot: format!("block_{}", "atk_small"),
                    prompt: "Choose blockers for Small".into(),
                    candidates: vec!["perm_blk_good".into(), "perm_blk_weak".into()],
                },
            ],
            ..action("a0", "declare_blockers", vec![])
        }]);
        view.you = "p0".into();
        view.battlefield = vec![
            Permanent {
                attacking: true,
                ..creature_perm(&format!("perm_{}", "atk_big"), "p1", 3, 3)
            },
            Permanent {
                attacking: true,
                ..creature_perm(&format!("perm_{}", "atk_small"), "p1", 2, 2)
            },
            creature_perm("perm_blk_good", "p0", 2, 4), // survives either attacker
            creature_perm("perm_blk_weak", "p0", 1, 1), // chump only — never used
        ];
        let _ = (attacker_big, attacker_small);

        let picked = choose_action(&view).unwrap();
        let answers = fill_answers(&view, picked).unwrap();
        let big = chosen_for(&answers, "block_atk_big");
        let small = chosen_for(&answers, "block_atk_small");
        // The good blocker is assigned to exactly one attacker; the weak one is
        // never thrown away, and no blocker is double-assigned.
        let all: Vec<&String> = big.iter().chain(small.iter()).collect();
        assert_eq!(all.iter().filter(|id| **id == "perm_blk_good").count(), 1);
        assert!(!all.iter().any(|id| **id == "perm_blk_weak"));
    }

    #[test]
    fn blockers_selection_respects_flying_evasion() {
        // CR 509.1b: a flying attacker can be blocked only by flying or reach. The
        // engine offers every untapped creature as a block candidate and enforces
        // evasion only on submission, so the agent must skip a ground blocker of a
        // flyer — declaring it would be illegal and re-offered forever.
        let with_keyword = |id: &str, controller: &str, p: i64, t: i64, kw: &str| Permanent {
            card: CardView {
                keywords: vec![kw.to_string()],
                ..creature_perm(id, controller, p, t).card
            },
            ..creature_perm(id, controller, p, t)
        };
        let flyer = Permanent {
            attacking: true,
            ..with_keyword("perm_atk", "p1", 4, 4, "flying")
        };
        // A 6/6 ground creature would survive the flyer (profitable by P/T) but cannot
        // legally block it.
        let ground = creature_perm("perm_ground", "p0", 6, 6);

        let block_action = |candidates: Vec<&str>| ValidAction {
            requirements: vec![TargetRequirement {
                slot: "block_atk".into(),
                prompt: "Choose blockers".into(),
                candidates: candidates.into_iter().map(str::to_string).collect(),
            }],
            ..action("a0", "declare_blockers", vec![])
        };

        // Only the ground blocker is a candidate: no legal block, so declare none.
        let mut view = view_with_actions(vec![block_action(vec!["perm_ground"])]);
        view.you = "p0".into();
        view.battlefield = vec![flyer.clone(), ground.clone()];
        let picked = choose_action(&view).unwrap();
        let answers = fill_answers(&view, picked).unwrap();
        assert!(
            chosen_for(&answers, "block_atk").is_empty(),
            "a ground creature cannot block a flyer"
        );

        // Add a profitable reacher: it is the legal, chosen blocker.
        let reacher = with_keyword("perm_reach", "p0", 5, 5, "reach"); // survives the 4/4 flyer
        let mut view2 = view_with_actions(vec![block_action(vec!["perm_ground", "perm_reach"])]);
        view2.you = "p0".into();
        view2.battlefield = vec![flyer, ground, reacher];
        let picked2 = choose_action(&view2).unwrap();
        let answers2 = fill_answers(&view2, picked2).unwrap();
        assert_eq!(
            chosen_for(&answers2, "block_atk"),
            ["perm_reach"],
            "a reach creature can block the flyer"
        );
    }

    #[test]
    fn fill_answers_option_prefers_keep_then_first() {
        let keep_first = Prompt::Option {
            slot: "s".into(),
            prompt: "?".into(),
            options: vec![
                PromptOption {
                    id: "mulligan".into(),
                    label: "Mulligan".into(),
                },
                PromptOption {
                    id: "keep".into(),
                    label: "Keep".into(),
                },
            ],
        };
        let view = view_with_actions(vec![]);
        let act = ValidAction {
            prompts: vec![keep_first],
            ..action("a0", "mulligan_decision", vec![])
        };
        let answers = fill_answers(&view, &act).unwrap();
        assert_eq!(chosen_for(&answers, "s"), ["keep"]);

        // With no "keep", the first option is the default.
        let act2 = ValidAction {
            prompts: vec![Prompt::Option {
                slot: "s".into(),
                prompt: "?".into(),
                options: vec![
                    PromptOption {
                        id: "a".into(),
                        label: "A".into(),
                    },
                    PromptOption {
                        id: "b".into(),
                        label: "B".into(),
                    },
                ],
            }],
            ..action("a0", "x", vec![])
        };
        assert_eq!(chosen_for(&fill_answers(&view, &act2).unwrap(), "s"), ["a"]);
    }

    #[test]
    fn fill_answers_order_keeps_items_as_given() {
        let view = view_with_actions(vec![]);
        let act = ValidAction {
            prompts: vec![Prompt::Order {
                slot: "ord".into(),
                prompt: "Order".into(),
                items: vec!["s1".into(), "s2".into(), "s3".into()],
            }],
            ..action("a0", "order", vec![])
        };
        let answers = fill_answers(&view, &act).unwrap();
        assert_eq!(chosen_for(&answers, "ord"), ["s1", "s2", "s3"]);
    }

    #[test]
    fn fill_answers_ability_target_prefers_an_opponent() {
        let mut view = view_with_actions(vec![]);
        view.you = "p0".into();
        view.opponents = vec![OpponentView {
            player_id: "p1".into(),
            hand_size: 0,
            life: 20,
            library_size: 0,
            graveyard_size: 0,
            statuses: vec![],
            eliminated: false,
        }];
        let act = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "t0".into(),
                prompt: "Choose target player".into(),
                candidates: vec!["p0".into(), "p1".into()],
            }],
            ..action("a0", "activate_ability", vec!["perm_x"])
        };
        let answers = fill_answers(&view, &act).unwrap();
        assert_eq!(chosen_for(&answers, "t0"), ["p1"]);
    }

    #[test]
    fn fill_answers_returns_none_when_a_mandatory_target_has_no_candidate() {
        let view = view_with_actions(vec![]);
        let act = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "t0".into(),
                prompt: "Choose target creature".into(),
                candidates: vec![],
            }],
            ..action("a0", "activate_ability", vec!["perm_x"])
        };
        assert!(fill_answers(&view, &act).is_none());
    }

    #[test]
    fn agent_never_submits_an_unadvertised_id_over_recorded_views() {
        // Property-style: over a battery of representative views, the agent's chosen
        // id is always offered and every filled id is one the slot advertised.
        let views = property_views();
        for view in &views {
            let Some(action) = choose_action(view) else {
                continue;
            };
            assert!(
                is_offered(view, &action.id),
                "chose an unoffered id in view {view:?}"
            );
            let Some(answers) = fill_answers(view, action) else {
                continue;
            };
            for answer in &answers {
                let advertised = advertised_ids(action, &answer.slot);
                for id in &answer.chosen {
                    assert!(
                        advertised.contains(id),
                        "slot {} chose un-advertised id {id} (advertised {advertised:?})",
                        answer.slot
                    );
                }
            }
        }
    }

    /// Every id a slot advertises (requirement candidates, option ids, zone
    /// candidates, or order items), for the property test above.
    fn advertised_ids(action: &ValidAction, slot: &str) -> Vec<String> {
        if let Some(req) = action.requirements.iter().find(|r| r.slot == slot) {
            return req.candidates.clone();
        }
        for prompt in &action.prompts {
            match prompt {
                Prompt::Option {
                    slot: s, options, ..
                } if s == slot => return options.iter().map(|o| o.id.clone()).collect(),
                Prompt::SelectFromZone {
                    slot: s,
                    candidates,
                    ..
                } if s == slot => return candidates.clone(),
                Prompt::Order { slot: s, items, .. } if s == slot => return items.clone(),
                _ => {}
            }
        }
        Vec::new()
    }

    /// A representative battery of views exercising every shape.
    fn property_views() -> Vec<GameView> {
        let mulligan = ValidAction {
            prompts: vec![Prompt::Option {
                slot: "decision".into(),
                prompt: "Keep or mulligan?".into(),
                options: vec![
                    PromptOption {
                        id: "keep".into(),
                        label: "Keep".into(),
                    },
                    PromptOption {
                        id: "mulligan".into(),
                        label: "Mulligan".into(),
                    },
                ],
            }],
            ..action("a0", "mulligan_decision", vec![])
        };
        let discard = ValidAction {
            prompts: vec![Prompt::SelectFromZone {
                slot: "discard".into(),
                prompt: "Choose a card to discard".into(),
                zone: "hand".into(),
                owner: "p0".into(),
                count: 1,
                candidates: vec!["card_1".into(), "card_2".into()],
            }],
            ..action("a0", "discard", vec![])
        };
        let attackers = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "attackers".into(),
                prompt: "Choose which creatures attack".into(),
                candidates: vec!["perm_a".into(), "perm_b".into()],
            }],
            ..action("a0", "declare_attackers", vec![])
        };
        let blockers = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "block_1".into(),
                prompt: "Choose blockers".into(),
                candidates: vec!["perm_x".into()],
            }],
            ..action("a0", "declare_blockers", vec![])
        };

        let mut discard_view = view_with_actions(vec![discard, action("a1", "concede", vec![])]);
        discard_view.my_hand = vec![
            hand_card("card_1", "Creature", "{G}"),
            hand_card("card_2", "Creature", "{4}{G}"),
        ];

        let mut attack_view = view_with_actions(vec![attackers, action("a1", "concede", vec![])]);
        attack_view.you = "p0".into();
        attack_view.battlefield = vec![
            creature_perm("perm_a", "p0", 2, 2),
            creature_perm("perm_b", "p0", 1, 1),
        ];

        let mut block_view = view_with_actions(vec![blockers, action("a1", "concede", vec![])]);
        block_view.you = "p0".into();
        block_view.battlefield = vec![
            Permanent {
                attacking: true,
                ..creature_perm("perm_1", "p1", 2, 2)
            },
            creature_perm("perm_x", "p0", 2, 3),
        ];

        vec![
            view_with_actions(vec![mulligan, action("a1", "concede", vec![])]),
            discard_view,
            attack_view,
            block_view,
            view_with_actions(vec![pass(), action("a1", "concede", vec![])]),
            view_with_actions(vec![]),
        ]
    }

    #[tokio::test]
    async fn rule_based_agent_is_deterministic_for_a_view() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "cast_spell", vec!["card_5"]),
            action("a2", "cast_spell", vec!["card_g"]),
            action("a3", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_5", "Creature", "{4}{G}"),
            hand_card("card_g", "Creature", "{G}"),
        ];
        let a = RuleBasedAgent.choose(&view).await.unwrap();
        let b = RuleBasedAgent.choose(&view).await.unwrap();
        assert_eq!(a, b);
        // The highest-cost creature is the deterministic pick.
        assert_eq!(a, "a1");
    }

    #[test]
    fn agent_config_parses_flag_env_and_timeout() {
        let flagged = AgentConfig::resolve(
            [
                "--agent".to_string(),
                "--agent-timeout".to_string(),
                "2.5".to_string(),
            ],
            |_| None,
        )
        .unwrap();
        assert!(flagged.enabled);
        assert_eq!(flagged.deadline, Duration::from_secs_f64(2.5));

        let eq_form = AgentConfig::resolve(["--agent-timeout=3".to_string()], |_| None).unwrap();
        assert_eq!(eq_form.deadline, Duration::from_secs(3));
        assert!(!eq_form.enabled);

        let default = AgentConfig::resolve(Vec::<String>::new(), |_| None).unwrap();
        assert_eq!(default, AgentConfig::default());

        let from_env = AgentConfig::resolve(Vec::<String>::new(), |key| {
            (key == AGENT_TIMEOUT_ENV_VAR).then(|| "4".to_string())
        })
        .unwrap();
        assert_eq!(from_env.deadline, Duration::from_secs(4));
    }

    #[test]
    fn agent_config_rejects_missing_or_invalid_timeout() {
        let missing = AgentConfig::resolve(["--agent-timeout".to_string()], |_| None).unwrap_err();
        assert_eq!(missing, ConfigError::MissingAgentTimeoutValue);

        let non_numeric =
            AgentConfig::resolve(["--agent-timeout=banana".to_string()], |_| None).unwrap_err();
        assert!(matches!(non_numeric, ConfigError::InvalidAgentTimeout(_)));

        let non_positive =
            AgentConfig::resolve(["--agent-timeout=0".to_string()], |_| None).unwrap_err();
        assert!(matches!(non_positive, ConfigError::InvalidAgentTimeout(_)));
    }
}
