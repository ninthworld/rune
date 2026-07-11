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
use rune_protocol::{ChooseAction, ClientMessage, GameView};
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

/// The built-in, deterministic, network-free agent: it passes priority when that
/// action is offered, otherwise takes the first offered action.
///
/// It is both the binary's default opponent (so `--agent` needs no provider or
/// secrets to run) and a ready-made stub for tests. It never stalls: every
/// actionable view yields exactly the [`safe_default`] choice.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassPriorityAgent;

impl Agent for PassPriorityAgent {
    async fn choose(&self, view: &GameView) -> Result<String, AgentError> {
        safe_default(view)
            .map(str::to_string)
            .ok_or_else(|| AgentError::Backend("no actions were offered".to_string()))
    }
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
/// action's content-binding `token` verbatim (ADR 0009). The pass-priority agent
/// only ever picks requirement-less actions; if a chosen action does carry target
/// requirements the agent has no way to fill, this falls back to a requirement-less
/// pass rather than send an answer the server would reject and re-offer forever.
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
    if action.requirements.is_empty() {
        return Ok(Some(ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: Vec::new(),
        }));
    }

    // The chosen action needs targets the pass agent cannot supply; substitute a
    // requirement-less pass so the game never stalls on a rejected answer.
    match view
        .valid_actions
        .iter()
        .find(|a| a.kind == PASS_PRIORITY_KIND && a.requirements.is_empty())
    {
        Some(pass) => {
            let note = format!(
                "agent: {:?} needs targets it cannot choose — passing instead\n",
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
                "agent: chosen action needs targets and no pass is available — skipping\n",
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
    use rune_protocol::{Phase, ValidAction};

    fn view_with_actions(actions: Vec<ValidAction>) -> GameView {
        GameView {
            you: "p0".into(),
            my_hand: vec![],
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::PrecombatMain,
            mana_pool: vec![],
            priority_player: Some("p0".into()),
            valid_actions: actions,
            action_deadline: None,
            result: None,
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
