//! Client-side **lobby flow** for the RUNE CLI (ADR 0012, issue #115).
//!
//! Before a game exists the connection speaks the pre-game pair: the server pushes
//! a full [`LobbyView`] on every change and the client sends a [`LobbyCommand`] to
//! act (create/join a room, submit a deck, ready up). This module is the terminal
//! client's half of that contract and — like the in-game loop — it is a **dumb
//! renderer**: it displays the `valid_commands` the server offered and echoes one
//! back, computing no legality of its own (`AGENTS.md`). A single `LobbyView`
//! reconstructs the whole pre-game display, so nothing is carried across frames.
//!
//! Two drivers sit on top of the same read/serialize plumbing:
//!
//! - [`run_lobby_interactive`] renders each `LobbyView` as numbered menus and reads
//!   the operator's choice from stdin (used by [`run_lobby_session`](crate::run_lobby_session)).
//! - [`run_lobby_agent`] drives the lobby unattended from a [`LobbyConfig`] parsed
//!   from `--agent`-mode flags, so an agent can be pointed at a room and reach the
//!   game with no human present (used by
//!   [`run_agent_lobby_session`](crate::run_agent_lobby_session)).
//!
//! Both return the first [`GameView`] the moment the ready gate passes: the server
//! switches the same socket from `LobbyView` JSON to `GameView` JSON, and the caller
//! hands that frame to the in-game loop. Neither driver holds any game logic.

use futures_util::{SinkExt, StreamExt};
use rune_protocol::{
    CreateRoom, GameView, JoinRoom, LobbyCommand, LobbyView, Ready, RoomConfig, SetName,
    SpectateRoom, SubmitDeck,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

use crate::{write_flush, write_str, ConfigError, SessionError, WsRead, WsWrite};

/// Default seat count for a room created with `--create` but no `--seats`. The
/// engine is two-player, so two seats is the natural default (ADR 0012).
const DEFAULT_SEATS: u8 = 2;

/// Default game-setup id when `--game-setup` is not given. The catalogue of setups
/// is owned by ADR 0013; the server treats the id as opaque and validates it, so the
/// CLI only needs a sensible placeholder here.
const DEFAULT_GAME_SETUP: &str = "standard_2p";

/// What an `--agent`-mode connection should do about a room: create one with a
/// config, or join an existing one by id. Absent means the plan named neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomAction {
    /// Create a new room with this configuration (`--create`).
    Create(RoomConfig),
    /// Join an existing room by its id (`--room <id>`).
    Join(String),
}

/// An unattended lobby plan parsed from `--agent`-mode flags: whether to create or
/// join a room, the deck to submit, and whether to ready up automatically once
/// decked. [`next_command`](LobbyConfig::next_command) turns a `LobbyView` into the
/// single next command that advances the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyConfig {
    /// Whether to create a room, join one, or (None) neither.
    pub action: Option<RoomAction>,
    /// The decklist (card identities) to submit for this connection's seat, if any.
    pub deck: Option<Vec<String>>,
    /// Whether to send `ready` automatically once the seat is decked.
    pub auto_ready: bool,
}

impl Default for LobbyConfig {
    fn default() -> Self {
        Self {
            action: None,
            deck: None,
            auto_ready: true,
        }
    }
}

impl LobbyConfig {
    /// Build a [`LobbyConfig`] from process arguments.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if a flag is missing its value, `--seats` is not a
    /// valid seat count, or both `--create` and `--room` are supplied.
    pub fn from_args() -> Result<Self, ConfigError> {
        Self::resolve(std::env::args().skip(1))
    }

    /// Core of [`LobbyConfig::from_args`], with arguments injected so it can be
    /// unit-tested without touching process globals.
    ///
    /// Flags: `--create` opens a room, `--seats <n>` (default 2) and `--game-setup
    /// <id>` (default `standard_2p`) configure it; `--room <id>` joins one instead;
    /// `--deck <a,b,c>` submits a decklist; `--no-auto-ready` disables readying up
    /// automatically (the default is to ready once decked).
    ///
    /// # Errors
    /// Returns [`ConfigError`] as described on [`from_args`](LobbyConfig::from_args).
    pub fn resolve<A>(args: A) -> Result<Self, ConfigError>
    where
        A: IntoIterator<Item = String>,
    {
        let mut create = false;
        let mut room: Option<String> = None;
        let mut seats: Option<String> = None;
        let mut game_setup: Option<String> = None;
        let mut deck: Option<String> = None;
        let mut auto_ready = true;

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--create" {
                create = true;
            } else if let Some(value) = arg.strip_prefix("--room=") {
                room = Some(value.to_string());
            } else if arg == "--room" {
                room = Some(args.next().ok_or(ConfigError::MissingRoomValue)?);
            } else if let Some(value) = arg.strip_prefix("--seats=") {
                seats = Some(value.to_string());
            } else if arg == "--seats" {
                seats = Some(args.next().ok_or(ConfigError::MissingSeatsValue)?);
            } else if let Some(value) = arg.strip_prefix("--game-setup=") {
                game_setup = Some(value.to_string());
            } else if arg == "--game-setup" {
                game_setup = Some(args.next().ok_or(ConfigError::MissingGameSetupValue)?);
            } else if let Some(value) = arg.strip_prefix("--deck=") {
                deck = Some(value.to_string());
            } else if arg == "--deck" {
                deck = Some(args.next().ok_or(ConfigError::MissingDeckValue)?);
            } else if arg == "--no-auto-ready" {
                auto_ready = false;
            }
        }

        if create && room.is_some() {
            return Err(ConfigError::ConflictingRoomAction);
        }

        let action = if let Some(room_id) = room {
            Some(RoomAction::Join(room_id))
        } else if create {
            let seats = match seats {
                Some(raw) => raw
                    .trim()
                    .parse::<u8>()
                    .map_err(|_| ConfigError::InvalidSeats(raw))?,
                None => DEFAULT_SEATS,
            };
            Some(RoomAction::Create(RoomConfig {
                seats,
                game_setup: game_setup.unwrap_or_else(|| DEFAULT_GAME_SETUP.to_string()),
            }))
        } else {
            None
        };

        Ok(Self {
            action,
            deck: deck.map(|raw| parse_deck(&raw)),
            auto_ready,
        })
    }

    /// The single next [`LobbyCommand`] this plan should send in response to `view`,
    /// or `None` if there is nothing to do yet (wait for the next view).
    ///
    /// The choice is derived purely from the authoritative `view` and gated on
    /// `view.valid_commands`, so it is idempotent: a view that repeats the same state
    /// yields the same command, and a view showing the plan already advanced (seated,
    /// decked, ready) yields the next step or `None`. The client computes no
    /// legality — it only picks among the commands the server said are legal.
    #[must_use]
    pub fn next_command(&self, view: &LobbyView) -> Option<LobbyCommand> {
        let offers = |kind: &str| view.valid_commands.iter().any(|c| c == kind);

        let Some(room) = &view.room else {
            return match &self.action {
                Some(RoomAction::Create(config)) if offers("create_room") => {
                    Some(LobbyCommand::CreateRoom(CreateRoom {
                        config: config.clone(),
                    }))
                }
                Some(RoomAction::Join(room_id)) if offers("join_room") => {
                    Some(LobbyCommand::JoinRoom(JoinRoom {
                        room_id: room_id.clone(),
                    }))
                }
                _ => None,
            };
        };

        // Seated: find our own seat by matching the public identity, then advance the
        // deck → ready pipeline one step at a time.
        let seat = room
            .seats
            .iter()
            .find(|seat| seat.occupied_by.as_deref() == Some(view.you.as_str()))?;
        if !seat.decked {
            return match &self.deck {
                Some(cards) if offers("submit_deck") => {
                    Some(LobbyCommand::SubmitDeck(SubmitDeck {
                        cards: cards.clone(),
                    }))
                }
                _ => None,
            };
        }
        if !seat.ready && self.auto_ready && offers("ready") {
            return Some(LobbyCommand::Ready(Ready { ready: true }));
        }
        None
    }
}

/// Split a `--deck`/menu decklist string into card identities: comma-separated,
/// trimmed, with empty entries dropped. Purely mechanical — the server validates the
/// identities authoritatively (ADR 0012).
#[must_use]
pub(crate) fn parse_deck(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// One decoded pre-game frame: either a lobby snapshot to act on, or the first game
/// view that signals the ready gate passed and the connection is now in a game.
enum LobbyFrame {
    /// A fresh full [`LobbyView`] to render/drive.
    Lobby(LobbyView),
    /// The hand-off: the server switched this socket to the in-game contract.
    /// Boxed: a full [`GameView`] is much larger than a [`LobbyView`], so keeping it
    /// behind a pointer keeps the enum small (clippy `large_enum_variant`).
    Game(Box<GameView>),
}

/// Read frames until a decodable pre-game frame arrives, returning `None` when the
/// server closes the connection. A game view is told apart from a lobby view by its
/// `phase` field (the same discriminator the server's own pre-game test uses):
/// present ⇒ `GameView`, absent ⇒ `LobbyView`. Undecodable text and non-text frames
/// are skipped.
async fn read_lobby_frame<S>(read: &mut WsRead<S>) -> Result<Option<LobbyFrame>, SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        match read.next().await {
            Some(Ok(Message::Text(text))) => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(text.as_str()) else {
                    continue;
                };
                if value.get("phase").is_some() {
                    if let Ok(view) = serde_json::from_value::<GameView>(value) {
                        return Ok(Some(LobbyFrame::Game(Box::new(view))));
                    }
                } else if let Ok(view) = serde_json::from_value::<LobbyView>(value) {
                    return Ok(Some(LobbyFrame::Lobby(view)));
                }
            }
            Some(Ok(Message::Close(_))) | None => return Ok(None),
            // Ping/pong/binary/raw frames carry no protocol message; ignore.
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(SessionError::WebSocket(error)),
        }
    }
}

/// Serialize and send one [`LobbyCommand`] over the split socket.
async fn send_command<S>(write: &mut WsWrite<S>, command: &LobbyCommand) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let json = serde_json::to_string(command).map_err(SessionError::Encode)?;
    write
        .send(Message::Text(json))
        .await
        .map_err(SessionError::WebSocket)
}

/// Drive the interactive lobby to the moment a game starts, returning its first
/// [`GameView`] (or `None` if the operator quits or the server closes first).
///
/// Each `LobbyView` is rendered as a numbered menu of `valid_commands`; the operator
/// picks one and answers any sub-prompts (seats/setup for create, a room id for
/// join, a decklist for submit). The chosen [`LobbyCommand`] is sent and the loop
/// waits for the next view.
pub(crate) async fn run_lobby_interactive<S, R, W>(
    write: &mut WsWrite<S>,
    read: &mut WsRead<S>,
    input: &mut R,
    output: &mut W,
) -> Result<Option<GameView>, SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        let view = match read_lobby_frame(read).await? {
            Some(LobbyFrame::Game(view)) => return Ok(Some(*view)),
            Some(LobbyFrame::Lobby(view)) => view,
            None => {
                write_str(output, "\nServer closed the connection. Goodbye.\n").await?;
                return Ok(None);
            }
        };

        write_str(output, &render_lobby(&view)).await?;
        if view.valid_commands.is_empty() {
            // Nothing to do (e.g. waiting on other seats): await the next view.
            continue;
        }

        // Prompt for a command number, then build it (with any sub-prompts).
        let command = loop {
            write_str(output, &command_prompt(view.valid_commands.len())).await?;
            output.flush().await.map_err(SessionError::Io)?;

            line.clear();
            let read_bytes = input.read_line(&mut line).await.map_err(SessionError::Io)?;
            if read_bytes == 0 {
                write_str(output, "\nEnd of input. Goodbye.\n").await?;
                let _ = write.close().await;
                return Ok(None);
            }
            match select_command(&view, &line) {
                Some(kind) => {
                    let kind = kind.to_string();
                    match build_command(&kind, input, output, &mut line).await? {
                        Some(command) => break command,
                        // EOF during a sub-prompt.
                        None if input_at_eof(&line) => {
                            write_str(output, "\nEnd of input. Goodbye.\n").await?;
                            let _ = write.close().await;
                            return Ok(None);
                        }
                        // Sub-prompt cancelled (e.g. empty room id): re-render the menu.
                        None => {
                            write_str(output, &render_lobby(&view)).await?;
                        }
                    }
                }
                None => write_str(output, &not_listed(&line)).await?,
            }
        };

        send_command(write, &command).await?;
    }
}

/// Drive the unattended agent lobby to the moment a game starts, returning its first
/// [`GameView`] (or `None` if the server closes before the gate passes).
///
/// On each `LobbyView` the [`LobbyConfig`] decides the single next command; if there
/// is one it is logged and sent, otherwise the driver waits. All decisions are gated
/// on the server's `valid_commands`, so the agent computes no legality.
pub(crate) async fn run_lobby_agent<S, W>(
    write: &mut WsWrite<S>,
    read: &mut WsRead<S>,
    log: &mut W,
    plan: &LobbyConfig,
) -> Result<Option<GameView>, SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    W: AsyncWrite + Unpin,
{
    loop {
        match read_lobby_frame(read).await? {
            Some(LobbyFrame::Game(view)) => {
                write_flush(log, "agent: game started; entering play.\n").await?;
                return Ok(Some(*view));
            }
            Some(LobbyFrame::Lobby(view)) => {
                if let Some(command) = plan.next_command(&view) {
                    let note = format!("agent: {}\n", describe_command(&command));
                    write_flush(log, &note).await?;
                    send_command(write, &command).await?;
                }
            }
            None => {
                write_flush(log, "agent: server closed before a game started.\n").await?;
                return Ok(None);
            }
        }
    }
}

/// Render the whole pre-game display for one [`LobbyView`]: the connection's public
/// identity, the room and its seat roster (occupant, decked, ready), and the numbered
/// `valid_commands` menu. A pure projection — it shows only what the server sent.
#[must_use]
pub fn render_lobby(view: &LobbyView) -> String {
    let mut out = String::new();
    out.push_str("\n============== LOBBY ==============\n");
    match &view.name {
        Some(name) => out.push_str(&format!("You: {} ({})\n", name, view.you)),
        None => out.push_str(&format!("You: {}\n", view.you)),
    }
    match &view.room {
        None => out.push_str("Room: (none) — create a room or join one by id.\n"),
        Some(room) => {
            out.push_str(&format!(
                "Room {}: {} seat(s), setup {}\n",
                room.room_id, room.config.seats, room.config.game_setup
            ));
            for seat in &room.seats {
                // Prefer the occupant's display name (issue #294), falling back to the
                // player id, then "(empty)" for an open seat.
                let occupant = match (&seat.name, &seat.occupied_by) {
                    (Some(name), Some(id)) => format!("{name} ({id})"),
                    (_, Some(id)) => id.clone(),
                    (_, None) => "(empty)".to_string(),
                };
                let decked = if seat.decked { "decked" } else { "no deck" };
                let ready = if seat.ready { "ready" } else { "not ready" };
                out.push_str(&format!(
                    "  seat {}: {} [{}, {}]\n",
                    seat.seat, occupant, decked, ready
                ));
            }
        }
    }

    if view.valid_commands.is_empty() {
        out.push_str("\nWaiting for the other players...\n");
    } else {
        out.push_str("\nCommands:\n");
        for (index, command) in view.valid_commands.iter().enumerate() {
            out.push_str(&format!("  {}) {}\n", index + 1, command_label(command)));
        }
    }
    out
}

/// Map a menu entry to one of the offered `valid_commands`, or `None` for a number
/// that names no listed command. 1-based, exactly like the in-game action menu.
#[must_use]
fn select_command<'a>(view: &'a LobbyView, input: &str) -> Option<&'a str> {
    let choice: usize = input.trim().parse().ok()?;
    let index = choice.checked_sub(1)?;
    view.valid_commands.get(index).map(String::as_str)
}

/// Build the [`LobbyCommand`] for a chosen command `kind`, running any sub-prompts
/// (seats/setup, room id, decklist). Returns `Ok(None)` if the sub-prompt hit EOF or
/// was cancelled (`line` distinguishes: empty ⇒ EOF, see [`input_at_eof`]).
async fn build_command<R, W>(
    kind: &str,
    input: &mut R,
    output: &mut W,
    line: &mut String,
) -> Result<Option<LobbyCommand>, SessionError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    match kind {
        "create_room" => {
            let Some(seats) = prompt_seats(input, output, line).await? else {
                return Ok(None);
            };
            let Some(game_setup) = prompt_line(
                input,
                output,
                line,
                &format!("Game setup (default {DEFAULT_GAME_SETUP}): "),
            )
            .await?
            else {
                return Ok(None);
            };
            let game_setup = if game_setup.is_empty() {
                DEFAULT_GAME_SETUP.to_string()
            } else {
                game_setup
            };
            Ok(Some(LobbyCommand::CreateRoom(CreateRoom {
                config: RoomConfig { seats, game_setup },
            })))
        }
        "join_room" => {
            let Some(room_id) = prompt_line(input, output, line, "Room id to join: ").await? else {
                return Ok(None);
            };
            if room_id.is_empty() {
                // Cancelled: leave `line` non-empty so the caller re-renders rather
                // than treating it as EOF.
                line.clear();
                line.push('\n');
                return Ok(None);
            }
            Ok(Some(LobbyCommand::JoinRoom(JoinRoom { room_id })))
        }
        "submit_deck" => {
            let Some(raw) = prompt_line(
                input,
                output,
                line,
                "Decklist (comma-separated card identities): ",
            )
            .await?
            else {
                return Ok(None);
            };
            Ok(Some(LobbyCommand::SubmitDeck(SubmitDeck {
                cards: parse_deck(&raw),
            })))
        }
        "set_name" => {
            let Some(name) = prompt_line(input, output, line, "Display name: ").await? else {
                return Ok(None);
            };
            if name.is_empty() {
                // Cancelled: leave `line` non-empty so the caller re-renders rather
                // than treating a blank entry as EOF.
                line.clear();
                line.push('\n');
                return Ok(None);
            }
            Ok(Some(LobbyCommand::SetName(SetName { name })))
        }
        "ready" => Ok(Some(LobbyCommand::Ready(Ready { ready: true }))),
        "unready" => Ok(Some(LobbyCommand::Ready(Ready { ready: false }))),
        "leave" => Ok(Some(LobbyCommand::Leave)),
        // Unknown command kinds are rendered but not actionable here; re-prompt.
        _ => {
            line.clear();
            line.push('\n');
            Ok(None)
        }
    }
}

/// Prompt for a seat count, defaulting to [`DEFAULT_SEATS`] on a blank line and
/// re-prompting a non-numeric entry. `Ok(None)` on EOF.
async fn prompt_seats<R, W>(
    input: &mut R,
    output: &mut W,
    line: &mut String,
) -> Result<Option<u8>, SessionError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    loop {
        let Some(entry) = prompt_line(
            input,
            output,
            line,
            &format!("Seats [2-8] (default {DEFAULT_SEATS}): "),
        )
        .await?
        else {
            return Ok(None);
        };
        if entry.is_empty() {
            return Ok(Some(DEFAULT_SEATS));
        }
        match entry.parse::<u8>() {
            Ok(seats) => return Ok(Some(seats)),
            Err(_) => write_str(output, &not_listed(&entry)).await?,
        }
    }
}

/// Write `label`, flush, and read one trimmed line. Returns `Ok(None)` on EOF (with
/// `line` left empty so the caller can tell EOF from a cancelled/blank entry).
async fn prompt_line<R, W>(
    input: &mut R,
    output: &mut W,
    line: &mut String,
    label: &str,
) -> Result<Option<String>, SessionError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_str(output, label).await?;
    output.flush().await.map_err(SessionError::Io)?;
    line.clear();
    let read_bytes = input.read_line(line).await.map_err(SessionError::Io)?;
    if read_bytes == 0 {
        line.clear();
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

/// Whether `line` indicates the reader reached EOF (left empty by [`prompt_line`]),
/// as opposed to a cancelled sub-prompt (which leaves a non-empty sentinel).
fn input_at_eof(line: &str) -> bool {
    line.is_empty()
}

/// The prompt shown before reading a lobby command number.
fn command_prompt(count: usize) -> String {
    format!("Choose a command [1-{count}] (Ctrl-D to quit): ")
}

/// The re-prompt note for an entry that names no listed menu item. Mirrors the
/// in-game menu's wording.
fn not_listed(line: &str) -> String {
    format!(
        "  '{}' is not a listed choice — enter a number from the menu.\n",
        line.trim()
    )
}

/// A friendly label for a `valid_commands` kind, falling back to the raw kind so an
/// unknown (newer) command is still shown rather than hidden.
fn command_label(kind: &str) -> &str {
    match kind {
        "create_room" => "Create a room",
        "join_room" => "Join a room by id",
        "set_name" => "Set your display name",
        "submit_deck" => "Submit a deck",
        "ready" => "Ready up",
        "unready" => "Cancel ready",
        "leave" => "Leave the room",
        other => other,
    }
}

/// A one-line description of a command the agent is about to send, for its log.
fn describe_command(command: &LobbyCommand) -> String {
    match command {
        LobbyCommand::CreateRoom(CreateRoom { config }) => {
            format!(
                "creating a {}-seat room ({})",
                config.seats, config.game_setup
            )
        }
        LobbyCommand::JoinRoom(JoinRoom { room_id }) => format!("joining room {room_id}"),
        LobbyCommand::SpectateRoom(SpectateRoom { room_id }) => {
            format!("spectating room {room_id}")
        }
        LobbyCommand::SubmitDeck(SubmitDeck { cards }) => {
            format!("submitting a {}-card deck", cards.len())
        }
        LobbyCommand::Ready(Ready { ready: true }) => "readying up".to_string(),
        LobbyCommand::Ready(Ready { ready: false }) => "cancelling ready".to_string(),
        LobbyCommand::SetName(SetName { name }) => format!("setting display name to {name:?}"),
        LobbyCommand::Leave => "leaving the room".to_string(),
        LobbyCommand::Hello(_) => "saying hello".to_string(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use rune_protocol::{RoomView, SeatView};

    fn roomless_view(commands: &[&str]) -> LobbyView {
        LobbyView {
            session: "s:secret".into(),
            you: "p0".into(),
            name: None,
            room: None,
            directory: vec![],
            valid_commands: commands.iter().map(|c| c.to_string()).collect(),
        }
    }

    fn seat(index: u8, occupant: Option<&str>, decked: bool, ready: bool) -> SeatView {
        SeatView {
            seat: index,
            occupied_by: occupant.map(str::to_string),
            name: None,
            decked,
            ready,
        }
    }

    fn seated_view(you: &str, seats: Vec<SeatView>, commands: &[&str]) -> LobbyView {
        LobbyView {
            session: "s:secret".into(),
            you: you.into(),
            name: None,
            room: Some(RoomView {
                room_id: "r0".into(),
                config: RoomConfig {
                    seats: seats.len() as u8,
                    game_setup: DEFAULT_GAME_SETUP.into(),
                },
                seats,
            }),
            directory: vec![],
            valid_commands: commands.iter().map(|c| c.to_string()).collect(),
        }
    }

    #[test]
    fn parse_deck_splits_trims_and_drops_empties() {
        assert_eq!(parse_deck("1,1,2, 3 ,,4"), vec!["1", "1", "2", "3", "4"]);
        assert!(parse_deck("   ").is_empty());
        assert!(parse_deck("").is_empty());
    }

    #[test]
    fn resolve_create_uses_defaults_and_overrides() {
        let default = LobbyConfig::resolve(["--create".to_string()]).unwrap();
        assert_eq!(
            default.action,
            Some(RoomAction::Create(RoomConfig {
                seats: DEFAULT_SEATS,
                game_setup: DEFAULT_GAME_SETUP.into(),
            }))
        );
        assert!(default.auto_ready);
        assert!(default.deck.is_none());

        let custom = LobbyConfig::resolve([
            "--create".to_string(),
            "--seats".to_string(),
            "4".to_string(),
            "--game-setup=commander".to_string(),
            "--deck".to_string(),
            "1,2,3".to_string(),
            "--no-auto-ready".to_string(),
        ])
        .unwrap();
        assert_eq!(
            custom.action,
            Some(RoomAction::Create(RoomConfig {
                seats: 4,
                game_setup: "commander".into(),
            }))
        );
        assert_eq!(
            custom.deck.as_deref(),
            Some(&["1".into(), "2".into(), "3".into()][..])
        );
        assert!(!custom.auto_ready);
    }

    #[test]
    fn resolve_join_takes_a_room_id() {
        let join = LobbyConfig::resolve(["--room".to_string(), "r7".to_string()]).unwrap();
        assert_eq!(join.action, Some(RoomAction::Join("r7".into())));
    }

    #[test]
    fn resolve_rejects_bad_flags() {
        assert_eq!(
            LobbyConfig::resolve(["--room".to_string()]).unwrap_err(),
            ConfigError::MissingRoomValue
        );
        assert_eq!(
            LobbyConfig::resolve(["--create".to_string(), "--seats=banana".to_string()])
                .unwrap_err(),
            ConfigError::InvalidSeats("banana".to_string())
        );
        assert_eq!(
            LobbyConfig::resolve([
                "--create".to_string(),
                "--room".to_string(),
                "r0".to_string()
            ])
            .unwrap_err(),
            ConfigError::ConflictingRoomAction
        );
    }

    #[test]
    fn next_command_creates_then_decks_then_readies() {
        let plan = LobbyConfig {
            action: Some(RoomAction::Create(RoomConfig {
                seats: 2,
                game_setup: DEFAULT_GAME_SETUP.into(),
            })),
            deck: Some(vec!["1".into(), "2".into()]),
            auto_ready: true,
        };

        // Roomless → create.
        let create = plan.next_command(&roomless_view(&["create_room", "join_room"]));
        assert!(matches!(create, Some(LobbyCommand::CreateRoom(_))));

        // Seated but undecked → submit the deck.
        let undecked = seated_view(
            "p0",
            vec![
                seat(0, Some("p0"), false, false),
                seat(1, None, false, false),
            ],
            &["submit_deck", "leave"],
        );
        assert!(matches!(
            plan.next_command(&undecked),
            Some(LobbyCommand::SubmitDeck(_))
        ));

        // Decked but not ready → ready up.
        let decked = seated_view(
            "p0",
            vec![
                seat(0, Some("p0"), true, false),
                seat(1, Some("p1"), true, false),
            ],
            &["submit_deck", "ready", "leave"],
        );
        assert!(matches!(
            plan.next_command(&decked),
            Some(LobbyCommand::Ready(Ready { ready: true }))
        ));

        // Already ready → nothing to do.
        let ready = seated_view(
            "p0",
            vec![
                seat(0, Some("p0"), true, true),
                seat(1, Some("p1"), true, false),
            ],
            &["submit_deck", "unready", "leave"],
        );
        assert!(plan.next_command(&ready).is_none());
    }

    #[test]
    fn next_command_joins_by_id_and_respects_valid_commands() {
        let plan = LobbyConfig {
            action: Some(RoomAction::Join("r0".into())),
            deck: Some(vec!["1".into()]),
            auto_ready: true,
        };
        let join = plan.next_command(&roomless_view(&["create_room", "join_room"]));
        assert_eq!(
            join,
            Some(LobbyCommand::JoinRoom(JoinRoom {
                room_id: "r0".into()
            }))
        );

        // If the server does not currently offer the command, do nothing.
        assert!(plan.next_command(&roomless_view(&[])).is_none());
    }

    #[test]
    fn next_command_without_a_deck_waits_rather_than_submitting() {
        let plan = LobbyConfig {
            action: Some(RoomAction::Join("r0".into())),
            deck: None,
            auto_ready: true,
        };
        let undecked = seated_view(
            "p0",
            vec![seat(0, Some("p0"), false, false)],
            &["submit_deck", "leave"],
        );
        assert!(plan.next_command(&undecked).is_none());
    }

    #[test]
    fn render_lobby_shows_roster_and_numbered_commands() {
        let view = seated_view(
            "p0",
            vec![
                seat(0, Some("p0"), true, false),
                seat(1, None, false, false),
            ],
            &["submit_deck", "ready", "leave"],
        );
        let text = render_lobby(&view);
        assert!(text.contains("Room r0"));
        assert!(text.contains("seat 0: p0 [decked, not ready]"));
        assert!(text.contains("seat 1: (empty)"));
        assert!(text.contains("1) Submit a deck"));
        assert!(text.contains("2) Ready up"));
    }

    #[test]
    fn select_command_maps_one_based_menu() {
        let view = roomless_view(&["create_room", "join_room"]);
        assert_eq!(select_command(&view, "1"), Some("create_room"));
        assert_eq!(select_command(&view, "2"), Some("join_room"));
        assert_eq!(select_command(&view, "0"), None);
        assert_eq!(select_command(&view, "3"), None);
        assert_eq!(select_command(&view, "x"), None);
    }
}
