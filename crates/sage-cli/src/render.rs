//! Turning a `GameView` into text, and one line of operator input back into something
//! the server offered. Nothing here decides legality: every selection is an index into a
//! list the server sent.

use super::*;

/// Map an operator's raw menu entry to the offered `action_id`, or `None` if it
/// is not a number naming a listed action.
///
/// The menu is 1-based: `"1"` selects `valid_actions[0]`. Anything that is not a
/// positive integer within range (blank, non-numeric, `0`, out-of-range) returns
/// `None`, so the caller can re-prompt. This performs **no** game logic — it only
/// indexes into the actions the server already offered.
#[must_use]
pub fn select_action<'a>(view: &'a GameView, input: &str) -> Option<&'a str> {
    selected_action(view, input).map(|action| action.id.as_str())
}

/// Map an operator's raw menu entry to the offered [`ValidAction`] itself, or `None`
/// if it is not a number naming a listed action. Like [`select_action`] but returns
/// the whole action so the caller can read its content-binding `token` and target
/// `requirements` (ADR 0004). Performs **no** game logic — it only indexes.
#[must_use]
pub fn selected_action<'a>(view: &'a GameView, input: &str) -> Option<&'a ValidAction> {
    let choice: usize = input.trim().parse().ok()?;
    let index = choice.checked_sub(1)?;
    view.valid_actions.get(index)
}

/// Map an operator's raw menu entry to one of a requirement slot's candidate entity
/// ids, or `None` if it is not a number naming a listed candidate. The menu is
/// 1-based, exactly like [`select_action`]; the client only indexes into the
/// candidates the server already advertised for this slot (ADR 0004 §Client).
#[must_use]
pub fn select_target<'a>(req: &'a TargetRequirement, input: &str) -> Option<&'a str> {
    let choice: usize = input.trim().parse().ok()?;
    let index = choice.checked_sub(1)?;
    req.candidates.get(index).map(String::as_str)
}

/// Render one target requirement slot: its prompt and its candidates as a numbered
/// menu. A pure projection of the slot — the client shows only the candidates the
/// server listed and derives no legality.
#[must_use]
pub(crate) fn render_requirement(req: &TargetRequirement) -> String {
    let mut out = format!("\n{}:\n", req.prompt);
    if req.candidates.is_empty() {
        out.push_str("  (no legal targets)\n");
    } else {
        for (index, candidate) in req.candidates.iter().enumerate() {
            out.push_str(&format!("  {}) {}\n", index + 1, candidate));
        }
    }
    out
}

/// The prompt shown before reading a target choice.
pub(crate) fn target_prompt(count: usize) -> String {
    format!("Choose a target [1-{count}] (Ctrl-D to quit): ")
}

/// The re-prompt note for an entry that names no listed menu item.
pub(crate) fn not_listed(line: &str) -> String {
    format!(
        "  '{}' is not a listed choice — enter a number from the menu.\n",
        line.trim()
    )
}

/// The display label for a player id (issue #294): the chosen display name when the
/// server sent one in [`GameView::player_names`], suffixed with the opaque id (which
/// actions and targeting still reference), else the bare id. Display-only — the
/// client parses nothing and derives no name it was not given.
fn player_label(view: &GameView, id: &str) -> String {
    match view.player_names.get(id) {
        Some(name) => format!("{name} ({id})"),
        None => id.to_string(),
    }
}

/// Render the whole display for one [`GameView`]: a plain-text summary of the
/// public and owned state followed by the numbered `valid_actions` menu.
///
/// This is a pure projection of the view — it shows only what the server sent and
/// derives nothing. The output is deterministic for a given view, which is what
/// lets a fresh frame fully reconstruct the display.
#[must_use]
pub fn render(view: &GameView) -> String {
    let mut out = String::new();
    out.push_str("\n========================================\n");
    out.push_str(&format!("Phase: {:?}\n", view.phase));
    match &view.priority_player {
        Some(player) => out.push_str(&format!("Priority: {}\n", player_label(view, player))),
        None => out.push_str("Priority: (none)\n"),
    }
    if !view.mana_pool.is_empty() {
        out.push_str(&format!("Mana pool: {}\n", view.mana_pool.join(" ")));
    }

    // The receiver's own public stats — life and library size — the same numbers shown
    // for each opponent below, so a player can read their own life in the terminal too
    // (issue #255). Graveyards are listed separately for every player.
    out.push_str(&format!(
        "You ({}): life {}, library {}\n",
        player_label(view, &view.you),
        view.me.life,
        view.me.library_size,
    ));

    out.push_str(&format!("Your hand ({}):\n", view.my_hand.len()));
    if view.my_hand.is_empty() {
        out.push_str("  (empty)\n");
    } else {
        for card in &view.my_hand {
            out.push_str(&format!("  - {}\n", card_line(card)));
        }
    }

    for opponent in &view.opponents {
        out.push_str(&format!(
            "Opponent {}: life {}, hand {}, library {}, graveyard {}\n",
            player_label(view, &opponent.player_id),
            opponent.life,
            opponent.hand_size,
            opponent.library_size,
            opponent.graveyard_size,
        ));
    }

    if !view.battlefield.is_empty() {
        out.push_str("Battlefield:\n");
        for perm in &view.battlefield {
            let tapped = if perm.tapped { " (tapped)" } else { "" };
            out.push_str(&format!(
                "  - {} [{}]{}\n",
                perm.card.name,
                player_label(view, &perm.controller),
                tapped
            ));
        }
    }

    if !view.stack.is_empty() {
        out.push_str("Stack (top last):\n");
        for item in &view.stack {
            out.push_str(&format!(
                "  - {} [{}]\n",
                item.description,
                player_label(view, &item.controller)
            ));
        }
    }

    for pile in &view.graveyards {
        out.push_str(&format!(
            "Graveyard {}: {} card(s)\n",
            player_label(view, &pile.player_id),
            pile.cards.len()
        ));
    }
    for pile in &view.exile {
        out.push_str(&format!(
            "Exile {}: {} card(s)\n",
            player_label(view, &pile.player_id),
            pile.cards.len()
        ));
    }

    if view.valid_actions.is_empty() {
        out.push_str("\nNo actions available — waiting for the other player...\n");
    } else {
        out.push_str("\nActions:\n");
        for (index, action) in view.valid_actions.iter().enumerate() {
            out.push_str(&format!("  {}) {}\n", index + 1, action.label));
        }
    }

    out
}

/// One line describing a card the viewer may see: name, cost, type, and P/T.
fn card_line(card: &CardView) -> String {
    let mut line = card.name.clone();
    if let Some(cost) = &card.mana_cost {
        line.push(' ');
        line.push_str(cost);
    }
    if !card.type_line.is_empty() {
        line.push_str(" — ");
        line.push_str(&card.type_line);
    }
    if let (Some(power), Some(toughness)) = (&card.power, &card.toughness) {
        line.push_str(&format!(" ({power}/{toughness})"));
    }
    line
}

/// The prompt shown before reading a menu choice.
pub(crate) fn prompt(count: usize) -> String {
    format!("Choose an action [1-{count}] (Ctrl-D to quit): ")
}

/// Write a whole string to `output`, mapping any I/O failure to [`SessionError`].
pub(crate) async fn write_str<W: AsyncWrite + Unpin>(
    output: &mut W,
    text: &str,
) -> Result<(), SessionError> {
    output
        .write_all(text.as_bytes())
        .await
        .map_err(SessionError::Io)
}

/// Write a whole string and flush it, mapping any I/O failure to [`SessionError`].
/// Used for log/marker lines that a reader may be waiting on immediately.
pub(crate) async fn write_flush<W: AsyncWrite + Unpin>(
    output: &mut W,
    text: &str,
) -> Result<(), SessionError> {
    write_str(output, text).await?;
    output.flush().await.map_err(SessionError::Io)
}
