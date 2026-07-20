//! Server-side **AI opponents** for lobby seats (issue #415).
//!
//! An AI opponent is a new kind of *seat occupant*: where a human seat is bridged to a
//! WebSocket by [`serve_connection`](crate::serve_connection), an AI seat is driven
//! in-process by [`serve_ai_seat`] from an [`AiPolicy`]. The driver is the exact
//! reactive loop a human client runs — receive this seat's personalized [`GameView`],
//! and, when it offers actions, send back a [`ChooseAction`] naming one of them — so an
//! AI plays through the **same** protocol path a human does: the room projects the view
//! ([`personalized_view`](crate::view)), and the returned action is re-validated by the
//! room's own [`resolve_action`](crate::view) before it is applied. The AI therefore
//! computes no rules of its own; it only *picks among* the actions the engine already
//! offered, exactly like the terminal agent (`rune-cli`).
//!
//! # Groundwork for stronger AI
//! The seam is the [`AiPolicy`] trait (`GameView -> ChooseAction`). The one shipped
//! implementation, [`RandomPolicy`], plays a uniformly random *legal* action each
//! decision — a deliberately simple placeholder. A future heuristic, search, or
//! LLM-backed policy implements the same trait and drops into [`policy_for`] with a new
//! [`AiKind`]; nothing in the lobby, room, or protocol changes. [`ai_options`] projects
//! the available kinds into the [`CatalogView`](rune_protocol::CatalogView) so a client
//! can offer them without hardcoding the set.
//!
//! # No wall-clock randomness (determinism)
//! The engine's only randomness is an injected seed (ADR 0014); the AI keeps that
//! property. [`RandomPolicy`] draws from a seeded [`SplitMix64`] stream, so an AI seat
//! built from a pinned game seed replays identically — the same reproducibility the
//! end-to-end suite relies on (issue #145). No `rand`/wall-clock dependency is added.

use std::future::Future;

use rune_protocol::{ChooseAction, ClientMessage, GameView, Prompt, TargetChoice, ValidAction};
use tokio::sync::watch;
use tracing::{info, warn};

use crate::room::{RoomHandle, RoomInput, Seat};

/// The `kind` string the server uses for the pass-priority action (mirrors
/// `rune-server`'s view projection). The random policy prefers it as a safe fallback.
const PASS_PRIORITY_KIND: &str = "pass_priority";

/// A policy that chooses one of the actions a [`GameView`] offers.
///
/// This is the extension seam for AI strength (issue #415): the driver
/// ([`serve_ai_seat`]) is generic over the policy, so the placeholder [`RandomPolicy`]
/// and any future smarter policy share the exact same play loop. An implementation holds
/// **no** game logic — it only selects among `view.valid_actions` and fills the chosen
/// action's slots from the candidates the server already enumerated; the room
/// re-validates the returned answer regardless.
pub trait AiPolicy: Send {
    /// Choose an action for `view`, which is guaranteed to offer at least one
    /// (`valid_actions` is non-empty). Returns the [`ChooseAction`] to send, or `None`
    /// only in the degenerate case that nothing offered can be answered — the driver
    /// then simply waits for the next view rather than sending an answer the room would
    /// reject.
    fn choose(&mut self, view: &GameView) -> Option<ChooseAction>;
}

/// The kinds of AI opponent the server can seat (issue #415). A small closed set today —
/// only [`AiKind::Random`] — but the enum is the dispatch point a future kind extends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiKind {
    /// Plays a uniformly random legal action each decision ([`RandomPolicy`]).
    Random,
}

impl AiKind {
    /// The stable wire id for this kind — the value an
    /// [`AddAi::kind`](rune_protocol::AddAi::kind) carries and a
    /// [`SeatView::ai`](rune_protocol::SeatView::ai) reports.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Random => "random",
        }
    }

    /// Resolve a wire id to a kind, or `None` if it names no supported AI. The lobby
    /// rejects an [`AddAi`](rune_protocol::AddAi) whose `kind` does not resolve here.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "random" => Some(Self::Random),
            _ => None,
        }
    }

    /// Every supported kind, in a stable order — the source of both [`ai_options`] and
    /// the server's set of dispatchable policies.
    #[must_use]
    pub fn all() -> &'static [AiKind] {
        &[AiKind::Random]
    }

    /// A short human-readable name for the seating UI (projected into [`ai_options`]).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Random => "Random",
        }
    }

    /// A one-line description of how this kind plays (projected into [`ai_options`]).
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Random => "Plays a random legal action each decision. A simple sparring partner.",
        }
    }
}

/// Build the policy for `kind`, seeded from `seed` so a pinned game seed replays the AI
/// identically (issue #145). The returned trait object is what [`serve_ai_seat`] drives.
#[must_use]
pub fn policy_for(kind: AiKind, seed: u64) -> Box<dyn AiPolicy> {
    match kind {
        AiKind::Random => Box::new(RandomPolicy::new(seed)),
    }
}

/// Project every supported [`AiKind`] into the wire [`AiOption`] list a
/// [`CatalogView`](rune_protocol::CatalogView) advertises (issue #415), so a client can
/// present the AI choices from server metadata rather than a hardcoded list.
#[must_use]
pub fn ai_options() -> Vec<rune_protocol::AiOption> {
    AiKind::all()
        .iter()
        .map(|kind| rune_protocol::AiOption {
            id: kind.id().to_string(),
            name: kind.name().to_string(),
            description: kind.description().to_string(),
        })
        .collect()
}

/// A tiny deterministic PRNG ([SplitMix64]) — enough entropy to pick among a handful of
/// offered actions, with no `rand`/wall-clock dependency so the engine's seed-only
/// randomness contract (ADR 0014) is preserved for AI too.
///
/// [SplitMix64]: https://prng.di.unimi.it/splitmix64.c
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n` (returns `0` for `n == 0`).
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// A fair coin.
    fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// A placeholder AI (issue #415) that plays a uniformly random **legal** action each
/// decision. It is intentionally weak — the point is the [`AiPolicy`] seam, not the play
/// — but it always makes a legal, game-advancing move, so it never stalls a room:
///
/// - At a mulligan decision it always **keeps** (never spiralling into a mulligan-to-death).
/// - Otherwise it picks a random offered action, avoiding a bare **concede** unless that is
///   the only thing on offer, then fills the action's slots from the server's own
///   candidate sets: a random subset of attackers, a random legal target for a required
///   target slot, a random discard/selection, no blocks (always legal), and combat-damage
///   order "as given". Because every choice is drawn from the candidates the engine already
///   enumerated, the room accepts the answer without a rejection round-trip.
///
/// Ties and choices are drawn from a seeded stream ([`SplitMix64`]), so a pinned game seed
/// reproduces the AI's play exactly.
pub struct RandomPolicy {
    rng: SplitMix64,
}

impl RandomPolicy {
    /// A random policy seeded from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: SplitMix64::new(seed),
        }
    }

    /// Build the [`ChooseAction`] for `action`, echoing its content-binding token and
    /// filling its slots. `None` if a mandatory slot has no candidate to fill.
    fn choose_action(&mut self, view: &GameView, action: &ValidAction) -> Option<ChooseAction> {
        let targets = self.fill_answers(view, action)?;
        Some(ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets,
        })
    }

    /// Fill every choice slot of `action` — its target `requirements` and [`Prompt`]
    /// slots — with a legal selection. `None` only when a mandatory slot (an ability
    /// target with no candidates, or an option with no options) cannot be answered, which
    /// the caller turns into a fallback. A plain action (no slots) yields an empty
    /// selection.
    ///
    /// Every chosen id is drawn from that slot's advertised candidates/options/items, so
    /// the answer is legal by construction and the room accepts it without a rejection.
    fn fill_answers(
        &mut self,
        _view: &GameView,
        action: &ValidAction,
    ) -> Option<Vec<TargetChoice>> {
        let mut out: Vec<TargetChoice> = Vec::with_capacity(action.requirements.len());

        for req in &action.requirements {
            let chosen: Vec<String> = if req.slot == "attackers" {
                // Attack with a random subset of the eligible attackers; any subset the
                // engine offered is a legal declaration (including the empty one).
                req.candidates
                    .iter()
                    .filter(|_| self.rng.coin())
                    .cloned()
                    .collect()
            } else if req.slot.starts_with("defend_") {
                // Multiplayer per-attacker defender choice: one random advertised
                // defender. Harmless for a creature we did not attack with (ignored).
                self.pick_one(&req.candidates).into_iter().collect()
            } else if req.slot.starts_with("block_") {
                // Declare no blocks: always legal, and it keeps the simple AI from ever
                // submitting an illegal (e.g. evasion-violating) block that would stall.
                Vec::new()
            } else if req.slot == "bottom" {
                // Mulligan bottoming (we always keep, so effectively unreachable): bottom
                // the required number of arbitrary cards.
                let count = leading_count(&req.prompt).unwrap_or(req.candidates.len());
                self.take_random(&req.candidates, count)
            } else {
                // A mandatory ability/spell target slot (`t0`, `t1`, …): one random legal
                // candidate. No candidate means the action cannot be answered.
                vec![self.pick_one(&req.candidates)?]
            };
            out.push(TargetChoice {
                slot: req.slot.clone(),
                chosen,
            });
        }

        for prompt in &action.prompts {
            let choice = match prompt {
                Prompt::Option { slot, options, .. } => {
                    // Keep at a mulligan decision; otherwise choose a random option.
                    let picked = options
                        .iter()
                        .find(|option| {
                            option.id.eq_ignore_ascii_case("keep")
                                || option.label.to_lowercase().contains("keep")
                        })
                        .or_else(|| options.get(self.rng.below(options.len())))?;
                    TargetChoice {
                        slot: slot.clone(),
                        chosen: vec![picked.id.clone()],
                    }
                }
                Prompt::SelectFromZone {
                    slot,
                    count,
                    candidates,
                    ..
                } => TargetChoice {
                    slot: slot.clone(),
                    chosen: self.take_random(candidates, *count as usize),
                },
                Prompt::Order { slot, items, .. } => TargetChoice {
                    slot: slot.clone(),
                    // "As given": a legal permutation with no ranking logic.
                    chosen: items.clone(),
                },
            };
            out.push(choice);
        }

        Some(out)
    }

    /// One random element of `candidates`, or `None` when empty.
    fn pick_one(&mut self, candidates: &[String]) -> Option<String> {
        if candidates.is_empty() {
            None
        } else {
            Some(candidates[self.rng.below(candidates.len())].clone())
        }
    }

    /// `count` distinct random elements of `candidates` (capped at what is available),
    /// drawn without replacement so a selection never repeats an id.
    fn take_random(&mut self, candidates: &[String], count: usize) -> Vec<String> {
        let mut pool: Vec<String> = candidates.to_vec();
        let n = count.min(pool.len());
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let idx = self.rng.below(pool.len());
            out.push(pool.swap_remove(idx));
        }
        out
    }
}

impl AiPolicy for RandomPolicy {
    fn choose(&mut self, view: &GameView) -> Option<ChooseAction> {
        let actions = &view.valid_actions;
        if actions.is_empty() {
            return None;
        }

        // Never randomly concede: choose from everything else, and only fall to concede
        // if it is genuinely the sole option (a forced window offering nothing else).
        let choosable: Vec<&ValidAction> = actions.iter().filter(|a| a.kind != "concede").collect();
        let pool = if choosable.is_empty() {
            actions.iter().collect::<Vec<_>>()
        } else {
            choosable
        };

        // Try a random action; if its slots cannot be filled, fall back to a pass, then to
        // the first fillable action, so a decision is always answered when one can be.
        let pick = pool[self.rng.below(pool.len())];
        if let Some(choose) = self.choose_action(view, pick) {
            return Some(choose);
        }
        if let Some(pass) = actions.iter().find(|a| a.kind == PASS_PRIORITY_KIND) {
            if let Some(choose) = self.choose_action(view, pass) {
                return Some(choose);
            }
        }
        actions
            .iter()
            .find_map(|action| self.choose_action(view, action))
    }
}

/// The leading integer of a prompt like `"Put 2 card(s) on the bottom …"` (→ `2`), or
/// `None` if it opens with no number. Lets a bottoming honor a count the wire requirement
/// does not carry as a field.
fn leading_count(prompt: &str) -> Option<usize> {
    prompt
        .split_whitespace()
        .find_map(|word| word.parse::<usize>().ok())
}

/// Drive an **AI seat** to completion over a running [`Room`](crate::Room) (issue #415).
///
/// This is the in-process sibling of [`serve_connection`](crate::serve_connection): it
/// joins the room for `seat` with its own latest-value outbox, then reacts to each pushed
/// [`GameView`] — when the view offers actions (this seat holds priority or owes a forced
/// choice), it asks `policy` for a [`ChooseAction`] and sends it back as a
/// [`RoomInput::Message`]; a view with no actions is skipped. It holds **no** game logic:
/// the room re-validates every answer through its own `resolve_action`, exactly as it does
/// a human's, so an AI can only ever take an action the engine offered.
///
/// The loop ends when the room drops this seat's outbox — the room task stops on game over
/// (or teardown) — after which the seat is released. `shutdown` lets an embedder stop the
/// driver early (server shutdown); pass [`std::future::pending`] for a driver that only
/// ends when the room does.
pub async fn serve_ai_seat<F>(
    seat: Seat,
    room: RoomHandle,
    mut policy: Box<dyn AiPolicy>,
    shutdown: F,
) where
    F: Future<Output = ()>,
{
    let (outbox_tx, mut outbox_rx) = watch::channel::<Option<GameView>>(None);
    if !room.send(RoomInput::Join {
        seat,
        outbox: outbox_tx,
    }) {
        warn!(seat, "room unavailable at AI join; not seating AI");
        return;
    }
    info!(seat, "AI opponent seated");

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            changed = outbox_rx.changed() => match changed {
                Ok(()) => {
                    let latest = outbox_rx.borrow_and_update().clone();
                    let Some(view) = latest else { continue };
                    // No actions offered: this seat does not hold priority — wait.
                    if view.valid_actions.is_empty() {
                        continue;
                    }
                    if let Some(choose) = policy.choose(&view) {
                        if !room.send(RoomInput::Message {
                            seat,
                            message: ClientMessage::ChooseAction(choose),
                        }) {
                            break;
                        }
                    }
                }
                // The room dropped our outbox (task stopped, e.g. game over): we are done.
                Err(_) => break,
            },
        }
    }

    let _ = room.send(RoomInput::Leave { seat });
    info!(seat, "AI opponent seat released");
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use rune_protocol::{Phase, PromptOption};

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
            command: vec![],
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
            commander_tax: Vec::new(),
            seat_order: Vec::new(),
        }
    }

    fn action(id: &str, kind: &str) -> ValidAction {
        ValidAction {
            id: id.into(),
            kind: kind.into(),
            label: kind.into(),
            subject: vec![],
            token: format!("tok_{id}"),
            ..Default::default()
        }
    }

    #[test]
    fn ai_kind_ids_round_trip() {
        for kind in AiKind::all() {
            assert_eq!(AiKind::from_id(kind.id()), Some(*kind));
        }
        assert_eq!(AiKind::from_id("nope"), None);
    }

    #[test]
    fn ai_options_project_every_kind() {
        let options = ai_options();
        assert_eq!(options.len(), AiKind::all().len());
        assert!(options.iter().any(|o| o.id == "random"));
        assert!(options.iter().all(|o| !o.name.is_empty()));
    }

    #[test]
    fn random_policy_chooses_an_offered_action() {
        let view = view_with_actions(vec![
            action("a0", PASS_PRIORITY_KIND),
            action("a1", "play_land"),
        ]);
        let mut policy = RandomPolicy::new(1);
        let choose = policy.choose(&view).expect("an action is chosen");
        assert!(
            view.valid_actions.iter().any(|a| a.id == choose.action_id),
            "the chosen id is one the view offered",
        );
        // The content-binding token is echoed verbatim (ADR 0009).
        let offered = view
            .valid_actions
            .iter()
            .find(|a| a.id == choose.action_id)
            .unwrap();
        assert_eq!(choose.token, offered.token);
    }

    #[test]
    fn random_policy_is_deterministic_for_a_seed() {
        let view = view_with_actions(vec![
            action("a0", PASS_PRIORITY_KIND),
            action("a1", "play_land"),
            action("a2", "cast_spell"),
        ]);
        let a: Vec<String> = (0..8)
            .map(|_| RandomPolicy::new(42).choose(&view).unwrap().action_id)
            .collect();
        // Same seed → same first choice every time (a fresh policy each call).
        assert!(a.iter().all(|id| *id == a[0]));

        // A single stream advances across calls, so it does not lock onto one action.
        let mut policy = RandomPolicy::new(42);
        let stream: Vec<String> = (0..20)
            .map(|_| policy.choose(&view).unwrap().action_id)
            .collect();
        assert!(
            stream.iter().any(|id| *id != stream[0]),
            "a running stream visits more than one action",
        );
    }

    #[test]
    fn random_policy_avoids_concede_when_anything_else_is_offered() {
        let view = view_with_actions(vec![action("a0", "pass_priority"), action("a1", "concede")]);
        let mut policy = RandomPolicy::new(7);
        // Over many draws it must never pick concede while a pass is available.
        for _ in 0..64 {
            let choose = policy.choose(&view).unwrap();
            assert_ne!(choose.action_id, "a1", "never randomly concedes");
        }
    }

    #[test]
    fn random_policy_takes_concede_when_it_is_the_only_action() {
        let view = view_with_actions(vec![action("a0", "concede")]);
        let mut policy = RandomPolicy::new(3);
        assert_eq!(policy.choose(&view).unwrap().action_id, "a0");
    }

    #[test]
    fn random_policy_keeps_at_a_mulligan_decision() {
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
            ..action("a0", "mulligan_decision")
        };
        let view = view_with_actions(vec![decision, action("a1", "concede")]);
        let mut policy = RandomPolicy::new(99);
        for _ in 0..32 {
            let choose = policy.choose(&view).unwrap();
            assert_eq!(choose.action_id, "a0");
            let decision_choice = choose
                .targets
                .iter()
                .find(|t| t.slot == "decision")
                .expect("the decision slot is filled");
            assert_eq!(decision_choice.chosen, vec!["keep".to_string()]);
        }
    }

    #[test]
    fn random_policy_returns_none_for_an_empty_view() {
        let view = view_with_actions(vec![]);
        assert!(RandomPolicy::new(1).choose(&view).is_none());
    }
}
