//! The personalized in-game [`GameView`] the server pushes after every change.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ActionAck, CardView, CommanderDamage, CommanderIdentity, CommanderTax, Emblem, GameLogEntry,
    GameResult, MatchFormat, OpponentView, Permanent, Phase, PlayerId, SelfView, StackItem,
    ValidAction, ZonePile,
};

/// The personalized state the server sends after every change (docs/protocol.md).
/// Hidden information is redacted server-side before this is built. A client must
/// be able to fully reconstruct its UI from a single `GameView` — no client state
/// is load-bearing across messages.
///
/// `Default` yields an **empty placeholder** view — no seat, no zones, the untap
/// step — which is not a meaningful game state and is never sent. It exists for the
/// same reason [`ValidAction`]'s does: every field is additive and optional except
/// `phase`, so a caller building a view field-by-field (test fixtures, the CLI's and
/// the AI's harnesses) should not have to restate a dozen empty collections, and
/// adding the *next* additive field should not touch a dozen unrelated literals.
/// The one literal that deliberately still enumerates every field is
/// `game_view_round_trips_through_json` in the tests module — it is the exhaustive
/// round-trip, so a new field must be considered there exactly once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameView {
    /// The receiver's own seat entity id (the `p{N}` form used for players
    /// throughout the view). Lets a client identify itself directly instead of
    /// inferring it from which id is not an opponent. `#[serde(default)]` so a
    /// payload from an older server that omits it still deserializes (to `""`).
    #[serde(default)]
    pub you: PlayerId,
    /// Full card objects for the receiving player only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub my_hand: Vec<CardView>,
    /// Cards from a **hidden zone that this receiver, and only this receiver, is
    /// currently being shown** — the cards a mid-resolution choice is asking them to
    /// pick from (issue #604).
    ///
    /// A player searching their library, looking at the top four, or reading an
    /// opponent's hand needs to see cards that are in no public pile and in no seat's
    /// `my_hand`. This is the one channel that carries them, and it is scoped as
    /// narrowly as the rules are: it holds exactly the candidates of the choice this
    /// seat is answering, it is absent from every other seat's view, and it is absent
    /// from the [`SpectatorView`](crate::SpectatorView) type entirely. A leak here
    /// would be a leaked deck, so the field exists *only* while a choice is owed and
    /// empties the moment it is answered.
    ///
    /// Purely a rendering channel: the ids match the choice prompt's `candidates`, so
    /// a client can show a picture of a card it is being asked about. It confers no
    /// interactivity of its own and grants no knowledge the prompt did not already.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revealed: Vec<CardView>,
    /// The receiver's own public stats (life total, library size) — see [`SelfView`].
    /// `#[serde(default)]` so a payload from an older server that omits it still
    /// deserializes (to a zero placeholder).
    #[serde(default)]
    pub me: SelfView,
    /// Redacted views of every other player.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opponents: Vec<OpponentView>,
    /// All permanents in play.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battlefield: Vec<Permanent>,
    /// The **emblems** in the game (CR 114, issue #620) — see [`Emblem`].
    ///
    /// Public information, so the list is the same in every seat's view and in the
    /// spectator's. Additive: omitted (and defaults to empty) in the overwhelming
    /// majority of games, where no planeswalker ultimate has resolved, so a client that
    /// ignores it sees no change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emblems: Vec<Emblem>,
    /// The stack, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackItem>,
    /// Each player's graveyard.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyards: Vec<ZonePile>,
    /// Each player's exile zone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile: Vec<ZonePile>,
    /// Each player's **command zone** (CR 903.6, issue #372): the public pile
    /// holding their commander while it is there. **Public information** — every
    /// seat sees every command zone. One [`ZonePile`] per player that has any card
    /// in their command zone; empty (and omitted from the wire) for a non-commander
    /// game or while every commander is elsewhere. Additive, like [`Self::exile`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<ZonePile>,
    /// The current turn step.
    pub phase: Phase,
    /// The current turn number (1-based; `0` only in an empty/default state). The
    /// server owns turn counting — the client never counts turns itself, it renders
    /// this. `#[serde(default)]` so a payload from an older server that omits it
    /// still deserializes (to `0`).
    #[serde(default)]
    pub turn: u32,
    /// The player whose turn it is (the *active player*), as the `p{N}` id used
    /// throughout the view. Distinct from [`Self::priority_player`]: the active
    /// player owns the turn even while priority sits with an opponent (e.g. during
    /// their response). `#[serde(default)]` so an older payload that omits it
    /// deserializes to `""` (unknown), and it is elided from the wire when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_player: PlayerId,
    /// The table's seat order: every player's id (`p0`, `p1`, …) in seat order,
    /// including the receiver and any eliminated players (issue #345). The explicit
    /// promise the multiplayer table layout relies on to place opponents in a stable
    /// arrangement around the receiver — opponents were only ever *happened* to be
    /// projected in seat order before, which no client could rely on. Additive:
    /// omitted (and defaults to empty) so a client that ignores it sees no change;
    /// a two-player client can continue to infer the arrangement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat_order: Vec<PlayerId>,
    /// The receiving player's unspent mana, as pip strings (e.g. `["{G}", "{G}"]`).
    /// Server-computed; the client only displays it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana_pool: Vec<String>,
    /// The player who currently holds priority, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_player: Option<PlayerId>,
    /// The only source of interactivity: what the receiving player may do now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_actions: Vec<ValidAction>,
    /// Seconds remaining for the pending decision, if a clock is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_deadline: Option<f64>,
    /// The terminal result once the game is over (winner/losers/reason, CR 104.2a).
    /// Omitted while the game is live (the empty-optional convention), so its
    /// presence alone tells a client the game has ended; when present,
    /// `valid_actions` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
    /// A bounded, sequence-numbered window of structured public game history.
    /// It is carried in every full view so reconnecting clients need no accumulated
    /// local log state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<GameLogEntry>,
    /// The receiver's own current **priority-stop preferences** (issue #264): the
    /// steps at which they want to receive priority even when the engine reports
    /// they have no meaningful action, so basic auto-pass (ADR 0010) does not skip
    /// them there. Carried on the view so the per-phase stops UI is reconstructable
    /// from a single message and survives reconnect (the preferences live on the
    /// room, like `player_names`, not in client memory). Per-viewer, not secret;
    /// the client renders toggles from this and answers with the `set_stops`
    /// message. Omitted from the wire when empty (stop nowhere — the default); a
    /// client treats a missing field as "no stops".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<Phase>,
    /// The receiver's **own-turn** priority stops (issue #455): the steps at which
    /// they want priority even when idle, but *only while they are the active
    /// player*. The narrower half of the same preference [`Self::stops`] carries —
    /// a step listed there stops on every turn and wins outright.
    ///
    /// This is the half that carries the human default: a seat the room considers
    /// human is seeded with its own main phases, so a turn never fast-forwards past
    /// the point where its owner would act, while the eleven other steps — and the
    /// whole of every opponent's turn — keep the ADR 0010 pacing. Set with
    /// `set_stops` alongside [`Self::stops`], stored on the room, and reflected here
    /// so the stops UI is reconstructable from a single message and survives
    /// reconnect. Omitted from the wire when empty; a client treats a missing field
    /// as "no own-turn stops".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub own_turn_stops: Vec<Phase>,
    /// Whether reaching this state **auto-passed** priority on the receiver's behalf
    /// (issue #264, ADR 0010): set on the broadcast that follows a settle in which
    /// the room passed priority for this seat, so the client can show a display-only
    /// "passed for you" indicator. Advisory and transient — the UI reconstructs
    /// fully without it, and a reconnect re-send need not preserve it. Omitted from
    /// the wire when `false`. Exactly `!auto_passed_steps.is_empty()`: the boolean
    /// summary of the list below, kept because a client may want only the summary.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub auto_passed: bool,
    /// **Where** the room acted on the receiver's behalf during the settle that
    /// produced this view (issue #455): the ordered path of turn-and-step positions
    /// it passed them through — see [`AutoPassedStep`].
    ///
    /// [`Self::auto_passed`] says a settle skipped you; this says where. That is the
    /// difference between "you were passed" and "you were passed at upkeep, draw and
    /// beginning of combat on turn 4", and it is the whole reason the field exists:
    /// ADR 0010's settle loop can advance a dozen steps between two broadcasts, and a
    /// client that only knows *that* it happened cannot tell a player what they did
    /// not get to see.
    ///
    /// A **path, not a set.** Consecutive entries for the same position collapse (one
    /// step opens a fresh priority window after every stack resolution, and saying so
    /// three times reads as three steps), but a position genuinely reached twice
    /// appears twice — which happens both across a turn boundary and *within* one
    /// turn, since an extra combat phase (CR 506.1) revisits the combat steps and an
    /// extra cleanup (CR 514.3a) revisits cleanup. Each entry therefore carries its
    /// own [`turn`](AutoPassedStep::turn) rather than leaving a client to infer a
    /// boundary from a repeat: that inference is wrong in exactly those cases, and
    /// inventing game information is what this field exists to stop.
    ///
    /// It names only positions the room acted at **for this receiver**; a step where
    /// another seat was passed is that seat's entry, not this one's.
    ///
    /// Advisory, transient, and display-only, exactly like [`Self::auto_passed`]:
    /// the UI reconstructs fully without it, `valid_actions` is unaffected, and a
    /// reconnect re-send need not preserve it. The authoritative record of *what
    /// happened* during a settle remains [`Self::log`] (ADR 0007), which carries the
    /// events themselves. Omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_passed_steps: Vec<AutoPassedStep>,
    /// Where in [`Self::log`] the receiver's unattended stretch **began** — the
    /// `sequence` of the first entry recorded since the last view they were sent.
    /// Every entry at or after it is something that happened while they were not
    /// being asked (issue #644).
    ///
    /// [`Self::auto_passed_steps`] says *where* the room acted and never *what
    /// happened there*, which is the half a player actually needs: a spell that was
    /// cast, resolved, and killed a creature inside one settle is three log events and
    /// zero steps a player would recognise. The log already carries those events; what
    /// it cannot say on its own is which of them the receiver missed, because that
    /// depends on when they were last shown anything. Only the room knows that, so the
    /// room says it.
    ///
    /// It marks the stretch from the **action that triggered the settle**, not from the
    /// settle's first pass: an opponent's spell is logged by their action, and a report
    /// that began after it would omit the very event the player is trying to understand.
    ///
    /// Present only when [`Self::auto_passed_steps`] is non-empty — with nothing passed
    /// for this seat there is no unattended stretch to describe. Advisory, transient,
    /// and display-only like the rest of this group: the UI reconstructs fully without
    /// it, and a client that cannot find the sequence in its log window simply shows
    /// the steps as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_passed_from: Option<u64>,
    /// Whether this view was pushed **because the receiver's last in-game action was
    /// rejected** (issue #265): a stale-view race meant the chosen action was no longer
    /// on offer (unknown id, mismatched [`ValidAction::token`], or a now-illegal target),
    /// so the server re-sent the current state unchanged rather than mutating the game.
    /// Purely advisory and transient — like [`Self::auto_passed`], the UI reconstructs
    /// fully without it and a reconnect re-send need not preserve it — so a client shows a
    /// brief, non-blaming "the game moved on" notice and nothing more. It is never load
    /// bearing: `valid_actions` already reflects the true current legal set. Set only on
    /// the one re-send that answers a rejection; omitted from the wire (treated as `false`)
    /// on every other broadcast.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub action_rejected: bool,
    /// The **acknowledgement** of the receiver's last submitted action (issue #554)
    /// — see [`ActionAck`]. Carried from the view that answers a
    /// [`ChooseAction`](crate::ChooseAction) bearing a
    /// [`submission`](crate::ChooseAction::submission) correlation id, and on that
    /// receiver's subsequent views until their next submission supersedes it. Only
    /// that receiver's views ever carry it.
    ///
    /// **Matched, not counted.** A client compares [`ActionAck::submission`] against
    /// the id it is still waiting on; a repeat of one it has already consumed names
    /// nothing and does nothing. Riding more than one view is what makes the ack
    /// survive a latest-value view channel, where a broadcast pushed while an earlier
    /// view is still in flight replaces it — an ack answering exactly one view would
    /// be lost to any unrelated broadcast that overtook it.
    ///
    /// Correspondingly, its **absence answers nothing**: an ordinary broadcast (another
    /// seat acting) is ack-less, so a client must not read one as the answer to its own
    /// click — that is precisely the race this field removes.
    ///
    /// It completes what [`Self::action_rejected`] could only half-say: that flag
    /// reports *that* a submission was refused but never *which*, so a client could
    /// not tell its own answer apart from a view caused by another seat. Transient
    /// and advisory — the UI reconstructs fully without it. Omitted (defaults to
    /// `None`) by an older server and for an uncorrelated submission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_ack: Option<ActionAck>,
    /// Public display names, keyed by [`PlayerId`] (issue #294): every player who has
    /// chosen a name maps to it, so any in-game surface — the turn indicator, player
    /// tiles, zone-browser titles, the game-over verdict — can label any player
    /// (`you`, an opponent, the active/priority player, a winner) without a lobby
    /// round-trip. Names are public information (no redaction beyond validation), the
    /// display name never replaces the `p{N}` id an action echoes back, and a player
    /// with no name simply has no entry here. Omitted from the wire when empty; a
    /// client treats a missing key as "unnamed" and falls back to a seat-derived
    /// label, so an older server that never sends names keeps working.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub player_names: BTreeMap<PlayerId, String>,
    /// Cumulative **combat** damage each commander has dealt each player this game
    /// (CR 903.10a, issue #371), one entry per `(commander, damaged)` pair that has
    /// taken any — see [`CommanderDamage`]. **Public information**, so it is the
    /// same for every receiver. A player who has taken 21+ from one commander has
    /// lost (that shows in [`Self::result`] with
    /// [`GameOverReason::CommanderDamage`]); the running tally lets a client warn
    /// before then. Additive: omitted (and defaults to empty) so a non-commander
    /// game — and an older client — is unchanged. Server-computed; never derived by
    /// the client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_damage: Vec<CommanderDamage>,
    /// The **commander tax** owed on each designated commander (CR 903.8, issue
    /// #372), one entry per player with a commander — see [`CommanderTax`]. **Public
    /// information**: the tax is `{2}` per prior cast from the command zone, so every
    /// seat sees how much a recast costs. Additive: omitted (and defaults to empty)
    /// for a non-commander game. Server-computed; never derived by the client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_tax: Vec<CommanderTax>,
    /// The **format** this match is played under (issue #553) — see [`MatchFormat`].
    /// The authoritative signal that a game is Commander, independent of whether any
    /// command zone, tax entry, or damage entry is currently populated: all three
    /// are legitimately empty in ordinary Commander states, so no client can infer
    /// the format from them. **Public information** (a room's format is advertised
    /// in the lobby), so it is the same for every receiver and for spectators.
    /// Room/session state, like [`Self::player_names`] — the room fills it in after
    /// projection. Additive: omitted (and defaults to `None`, read as "unknown
    /// format, not Commander") so an older server and an older client are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<MatchFormat>,
    /// Each seat's **commander identity** (CR 903.3/903.4, issue #553): the
    /// commander's display name and color identity, one entry per player who
    /// designated one — see [`CommanderIdentity`]. Keyed to the designation, so it
    /// is **stable for the whole game** and does not change when the commander is
    /// cast, dies, is exiled, or returns to the command zone; the `command` pile,
    /// the only previous source, disappears the moment the commander leaves it.
    /// **Public information**, so every receiver and every spectator sees the same
    /// list. Additive: omitted (and defaults to empty) for a non-commander game.
    /// Server-computed; never derived by the client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_identity: Vec<CommanderIdentity>,
    /// What **undo** can do at this table right now (issue #648) — see [`UndoView`].
    ///
    /// Present exactly when the room was configured with
    /// [`RoomConfig::undo_enabled`](crate::RoomConfig), so its *presence* answers "does
    /// this table allow taking an action back" and its
    /// [`available`](UndoView::available) answers "is there anything left to take
    /// back". A client draws no undo control without it and computes neither answer:
    /// how many checkpoints survive is server state, and a client that counted
    /// transitions itself would be keeping load-bearing history across messages.
    ///
    /// Public and identical for every seat — undo is a table rule, and any player may
    /// use it — so it is not personalized the way [`Self::stops`] is. Omitted (and
    /// defaulting to `None`) for every room that did not enable undo, which is every
    /// room by default and every room an older server serves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<UndoView>,
}

/// What an **undo** would do at this table right now (issue #648): the availability
/// half of [`GameView::undo`], stated by the server so the client can draw a control
/// without deriving anything.
///
/// The counts are *checkpoints*, not clicks: one checkpoint is one server-accepted
/// transition, so restoring one takes back a whole action — including whatever the
/// room's automation settled after it — and never half of one. Repeated undos walk
/// back through what is left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoView {
    /// How many earlier checkpoints are still restorable. `0` means the control is
    /// drawn unavailable: this is the oldest state the room still holds, either
    /// because nothing has happened yet or because the rest fell off the far end of
    /// [`limit`](Self::limit).
    pub available: u32,
    /// The most checkpoints the room will ever hold at once.
    ///
    /// On the wire because history is bounded and a bound a client cannot see is a
    /// bound it would misreport: "3 left" means something different at a limit of 3
    /// than at a limit of 50, and a client that assumed the game's whole history was
    /// undoable would promise a rollback the server cannot perform.
    pub limit: u32,
}

/// One position a settle passed the receiver through (issue #455): a step, and the
/// turn that step belonged to. An entry in [`GameView::auto_passed_steps`].
///
/// **Why the turn rides along.** The path is ordered and may repeat a step, and the
/// obvious reading of a repeat — "the settle crossed into a new turn" — is simply
/// wrong twice over: an extra combat phase (CR 506.1) revisits the combat steps
/// inside one turn, and an extra cleanup step (CR 514.3a) revisits cleanup. A client
/// that inferred a turn boundary from a repeated phase would be asserting game
/// structure the server never stated, which the client is not allowed to do. So the
/// server states it. With `turn` present, a presentation can group the path into
/// per-turn runs, keep every occurrence in order, and say where a boundary actually
/// fell — all of it read off the one view, none of it inferred.
///
/// The **active player** is deliberately *not* carried. This is an indicator that
/// refines [`GameView::auto_passed`], not a second game log: the authoritative record
/// of whose turn it was, and of everything that happened during it, is the
/// `step_changed` entry in [`GameView::log`] (ADR 0007), which already carries turn,
/// active player, and phase together.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoPassedStep {
    /// The step the room acted at.
    pub phase: Phase,
    /// The turn number that step belonged to. Present on every entry — including a
    /// single-entry path — so a client never has to decide whether it may read one
    /// entry's turn as another's.
    pub turn: u32,
}

#[cfg(test)]
mod tests;
