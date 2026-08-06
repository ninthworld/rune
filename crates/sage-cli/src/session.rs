//! The session loop: dialling the server, and driving one connection through the lobby
//! and then the game — reading a numbered choice from the operator and echoing back the
//! `action_id` the server already issued.

use super::*;

/// Dial the server named by `config` and complete the WebSocket handshake.
///
/// # Errors
/// Returns [`SessionError::WebSocket`] if the connection or handshake fails.
pub async fn connect(
    config: &CliConfig,
) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, SessionError>
{
    let url = config.ws_url();
    let (ws, _response) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(SessionError::WebSocket)?;
    Ok(ws)
}

/// Run the interactive session to completion over an already-connected socket.
///
/// The loop is: receive one [`GameView`], render it (summary + numbered menu of
/// `valid_actions`), and — only when the view offers actions — prompt for a menu
/// number and send the matching `action_id` as a [`ClientMessage::ChooseAction`].
/// A view with no actions is displayed and the loop simply waits for the next one.
///
/// `input` is the operator's line source (stdin in the binary) and `output` is
/// where the rendered display and prompts are written (stdout). Both are injected
/// so the loop can be driven by a test fixture.
///
/// The loop exits cleanly — returning `Ok(())` — when the server closes the
/// connection or when `input` reaches EOF; it never panics on either. It returns
/// an error only if the transport or a local write fails mid-session.
///
/// # Errors
/// Returns a [`SessionError`] if a WebSocket read/write, a stdout write, or the
/// encoding of a chosen action fails.
pub async fn run_session<S, R, W>(
    ws: WebSocketStream<S>,
    mut input: R,
    mut output: W,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    game_loop(&mut write, &mut read, &mut input, &mut output, None).await
}

/// Run the full interactive flow over an already-connected socket: the lobby
/// (create/join a room, submit a deck, ready) rendered as numbered menus, then the
/// in-game loop once the server constructs the game.
///
/// A single `LobbyView` reconstructs the whole pre-game display; the client renders
/// exactly the `valid_commands` the server offered and computes no legality. The
/// instant the ready gate passes the server pushes the first `GameView` on the *same
/// socket*, and this function hands off to [`game_loop`] with that view.
///
/// # Errors
/// Returns a [`SessionError`] if a WebSocket read/write, a stdout write, or the
/// encoding of a command/action fails.
pub async fn run_lobby_session<S, R, W>(
    ws: WebSocketStream<S>,
    mut input: R,
    mut output: W,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    match lobby::run_lobby_interactive(&mut write, &mut read, &mut input, &mut output).await? {
        Some(first_view) => {
            write_str(&mut output, "\n=== Game starting! ===\n").await?;
            game_loop(
                &mut write,
                &mut read,
                &mut input,
                &mut output,
                Some(first_view),
            )
            .await
        }
        None => Ok(()),
    }
}

/// The in-game loop over a split socket: render each `GameView`, and — when the view
/// offers actions — prompt for a menu number, fill any target `requirements`, and
/// send the matching action id, its content-binding `token`, and the chosen
/// `targets` (ADR 0004). `first_view` lets the lobby hand off the very first game
/// frame it already read; `None` starts by reading one.
pub(crate) async fn game_loop<S, R, W>(
    write: &mut WsWrite<S>,
    read: &mut WsRead<S>,
    input: &mut R,
    output: &mut W,
    first_view: Option<GameView>,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();
    let mut pending = first_view;

    'session: loop {
        // 1. Receive the next personalized view. The entire display is rebuilt from
        //    this single message — nothing is carried across frames.
        let view = match pending.take() {
            Some(view) => view,
            None => match next_game_view(read, output).await? {
                Some(view) => view,
                None => break 'session,
            },
        };

        write_str(output, &render(&view)).await?;

        // 2. No actions offered (we do not hold priority): await the next view.
        if view.valid_actions.is_empty() {
            continue;
        }

        // 3. Prompt until a valid menu number is entered, or stdin hits EOF, then
        //    fill any target requirements the chosen action carries.
        let (action_id, token, targets) = loop {
            write_str(output, &prompt(view.valid_actions.len())).await?;
            output.flush().await.map_err(SessionError::Io)?;

            line.clear();
            let read_bytes = input.read_line(&mut line).await.map_err(SessionError::Io)?;
            if read_bytes == 0 {
                write_str(output, "\nEnd of input. Goodbye.\n").await?;
                let _ = write.send(Message::Close(None)).await;
                return Ok(());
            }

            match selected_action(&view, &line) {
                Some(action) => match prompt_targets(action, input, output, &mut line).await? {
                    Some(targets) => break (action.id.clone(), action.token.clone(), targets),
                    None => {
                        write_str(output, "\nEnd of input. Goodbye.\n").await?;
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                },
                None => write_str(output, &not_listed(&line)).await?,
            }
        };

        // 4. Echo the chosen action id, its content-binding token (verbatim), and the
        //    atomically chosen targets; the server verifies the token against the
        //    action it currently offers and checks each target (ADR 0004).
        let choose = ClientMessage::ChooseAction(ChooseAction {
            action_id,
            token,
            targets,
            // The terminal client sends one action at a time and blocks on the reply,
            // so it needs no submission correlation id (issue #554).
            ..Default::default()
        });
        let json = serde_json::to_string(&choose).map_err(SessionError::Encode)?;
        write
            .send(Message::Text(json))
            .await
            .map_err(SessionError::WebSocket)?;
    }

    let _ = write.send(Message::Close(None)).await;
    Ok(())
}

/// Read frames until the next decodable [`GameView`] arrives, returning `None` when
/// the server closes the connection. Undecodable text frames are noted and skipped;
/// ping/pong/binary frames are ignored.
async fn next_game_view<S, W>(
    read: &mut WsRead<S>,
    output: &mut W,
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
                        let note = format!("! ignoring undecodable server message: {error}\n");
                        write_str(output, &note).await?;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => {
                write_str(output, "\nServer closed the connection. Goodbye.\n").await?;
                return Ok(None);
            }
            // Ping/pong/binary/raw frames carry no protocol message; ignore.
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(SessionError::WebSocket(error)),
        }
    }
}

/// Walk a chosen action's `requirements` and then its `prompts` as one prompt queue,
/// returning one [`TargetChoice`] per slot (ADR 0004, issue #156). Target slots are
/// filled from their advertised `candidates`; the option / select-from-zone / order
/// prompt slots are answered minimally (see [`prompt_choice`]). Returns `Ok(None)` if
/// stdin hits EOF mid-selection. An action with neither returns an empty selection
/// without prompting, so plain actions are unchanged.
async fn prompt_targets<R, W>(
    action: &ValidAction,
    input: &mut R,
    output: &mut W,
    line: &mut String,
) -> Result<Option<Vec<TargetChoice>>, SessionError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut targets = Vec::with_capacity(action.requirements.len() + action.prompts.len());
    for req in &action.requirements {
        write_str(output, &render_requirement(req)).await?;
        let chosen = loop {
            write_str(output, &target_prompt(req.candidates.len())).await?;
            output.flush().await.map_err(SessionError::Io)?;

            line.clear();
            let read_bytes = input.read_line(line).await.map_err(SessionError::Io)?;
            if read_bytes == 0 {
                return Ok(None);
            }
            match select_target(req, line) {
                Some(id) => break id.to_string(),
                None => write_str(output, &not_listed(line)).await?,
            }
        };
        targets.push(TargetChoice {
            slot: req.slot.clone(),
            chosen: vec![chosen],
        });
    }
    for prompt in &action.prompts {
        // A `pay_mana` slot is left unanswered on purpose: the server auto-pays any pip
        // this client did not fill (ADR 0010 — the policy is the server's), which is
        // exactly the behaviour the terminal client has always had. Assembling a payment
        // by hand is the web client's job, and asking for it here would be a numbered
        // list per pip in a client whose point is being quick to drive.
        if matches!(prompt, Prompt::PayMana { .. }) {
            continue;
        }
        match prompt_choice(prompt, input, output, line).await? {
            Some(choice) => targets.push(choice),
            None => return Ok(None),
        }
    }
    Ok(Some(targets))
}

/// Answer one non-target [`Prompt`] slot (issue #156), returning its
/// [`TargetChoice`] or `Ok(None)` on EOF. An `option` slot is a numbered choice; a
/// `select_from_zone` slot reads its `count` cards from the listed candidates; an
/// `order` slot is submitted in the order given (the terminal client offers no
/// reordering UI — that is the web client's job, issue #157). The client only ever
/// offers ids the server listed and computes no legality.
///
/// A `pay_mana` slot never reaches here — the caller skips it and lets the server pay.
async fn prompt_choice<R, W>(
    prompt: &Prompt,
    input: &mut R,
    output: &mut W,
    line: &mut String,
) -> Result<Option<TargetChoice>, SessionError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    match prompt {
        Prompt::Option {
            slot,
            prompt: text,
            options,
        } => {
            write_str(output, &render_options(text, options)).await?;
            let chosen = loop {
                write_str(output, &choice_prompt(options.len())).await?;
                output.flush().await.map_err(SessionError::Io)?;
                line.clear();
                if input.read_line(line).await.map_err(SessionError::Io)? == 0 {
                    return Ok(None);
                }
                match option_at(options, line) {
                    Some(id) => break id.to_string(),
                    None => write_str(output, &not_listed(line)).await?,
                }
            };
            Ok(Some(TargetChoice {
                slot: slot.clone(),
                chosen: vec![chosen],
            }))
        }
        Prompt::SelectFromZone {
            slot,
            prompt: text,
            count,
            min,
            candidates,
            ..
        } => {
            // `count` is the maximum; `min` (absent means "exactly `count`") is the
            // fewest the server will accept. Once the minimum is met a blank line stops
            // early, which is how a player scries nothing or declines to take a card.
            let floor = min.unwrap_or(*count);
            write_str(output, &render_candidates(text, candidates, floor, *count)).await?;
            let mut chosen = Vec::with_capacity(*count as usize);
            for which in 1..=*count {
                let id = loop {
                    write_str(
                        output,
                        &nth_card_prompt(candidates.len(), which, *count, which > floor),
                    )
                    .await?;
                    output.flush().await.map_err(SessionError::Io)?;
                    line.clear();
                    if input.read_line(line).await.map_err(SessionError::Io)? == 0 {
                        return Ok(None);
                    }
                    if which > floor && line.trim().is_empty() {
                        return Ok(Some(TargetChoice {
                            slot: slot.clone(),
                            chosen,
                        }));
                    }
                    match candidate_at(candidates, line) {
                        Some(id) => break id.to_string(),
                        None => write_str(output, &not_listed(line)).await?,
                    }
                };
                chosen.push(id);
            }
            Ok(Some(TargetChoice {
                slot: slot.clone(),
                chosen,
            }))
        }
        Prompt::Order {
            slot,
            prompt: text,
            items,
        } => {
            write_str(
                output,
                &format!("\n{text}: submitting in the listed order.\n"),
            )
            .await?;
            Ok(Some(TargetChoice {
                slot: slot.clone(),
                chosen: items.clone(),
            }))
        }
        // A numeric slot (issue #554): read a value inside the server's inclusive
        // range and answer with its decimal string. The bounds are the server's, so
        // the terminal client only re-prompts until the input lands inside them.
        Prompt::Number {
            slot,
            prompt: text,
            min,
            max,
            values,
        } => {
            write_str(output, &format!("\n{text} ({min}-{max}):\n")).await?;
            // Where the server enumerated what each value costs (the X of a mana cost),
            // show the price beside the number. The client prints the strings it was
            // handed and works nothing out.
            for option in values {
                write_str(output, &format!("  {} — {}\n", option.value, option.cost)).await?;
            }
            let chosen = loop {
                write_str(output, "> ").await?;
                output.flush().await.map_err(SessionError::Io)?;
                line.clear();
                if input.read_line(line).await.map_err(SessionError::Io)? == 0 {
                    return Ok(None);
                }
                match line.trim().parse::<u32>() {
                    Ok(value) if (*min..=*max).contains(&value) => break value,
                    _ => write_str(output, &not_listed(line)).await?,
                }
            };
            Ok(Some(TargetChoice {
                slot: slot.clone(),
                chosen: vec![chosen.to_string()],
            }))
        }
        // Skipped by the caller, which is why this asks nothing rather than asking
        // badly: an unanswered pip is one the server pays.
        Prompt::PayMana { slot, .. } => Ok(Some(TargetChoice {
            slot: slot.clone(),
            chosen: Vec::new(),
        })),
    }
}

/// Render an `option` prompt's named choices as a numbered menu.
fn render_options(text: &str, options: &[PromptOption]) -> String {
    let mut out = format!("\n{text}:\n");
    for (index, option) in options.iter().enumerate() {
        out.push_str(&format!("  {}) {}\n", index + 1, option.label));
    }
    out
}

/// Map a 1-based menu entry onto an `option` choice's id, or `None` if it names no
/// listed option.
fn option_at<'a>(options: &'a [PromptOption], input: &str) -> Option<&'a str> {
    let choice: usize = input.trim().parse().ok()?;
    options
        .get(choice.checked_sub(1)?)
        .map(|option| option.id.as_str())
}

/// The prompt shown before reading an `option` choice.
fn choice_prompt(count: usize) -> String {
    format!("Choose [1-{count}] (Ctrl-D to quit): ")
}

/// Render a `select_from_zone` prompt's candidate ids as a numbered menu.
fn render_candidates(text: &str, candidates: &[String], min: u32, count: u32) -> String {
    let how_many = if min == count {
        format!("choose {count}")
    } else {
        format!("choose {min}-{count}")
    };
    let mut out = format!("\n{text} ({how_many}):\n");
    for (index, candidate) in candidates.iter().enumerate() {
        out.push_str(&format!("  {}) {}\n", index + 1, candidate));
    }
    out
}

/// Map a 1-based menu entry onto a `select_from_zone` candidate id.
fn candidate_at<'a>(candidates: &'a [String], input: &str) -> Option<&'a str> {
    let choice: usize = input.trim().parse().ok()?;
    candidates.get(choice.checked_sub(1)?).map(String::as_str)
}

/// The prompt shown before reading the `which`-of-`total` card of a select-from-zone.
/// `optional` marks a pick past the server's stated minimum, where a blank line ends
/// the selection instead of being a mistake.
fn nth_card_prompt(count: usize, which: u32, total: u32, optional: bool) -> String {
    let stop = if optional { ", blank to stop" } else { "" };
    format!("Select card {which} of {total} [1-{count}]{stop} (Ctrl-D to quit): ")
}
