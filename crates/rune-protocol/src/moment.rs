//! Presentation moments — the bounded, ordered window of display-only pacing cues
//! that rides every view (issue #594).

use serde::{Deserialize, Serialize};

use crate::{
    AutoPassedStep, CardView, EntityId, GameOverReason, GameResult, LogBlock, LogDamageTarget,
    Phase, PlayerId,
};

/// The maximum number of [`PresentationMoment`]s one view carries (issue #594).
///
/// The window is **bounded on purpose**, and the bound is part of the contract rather
/// than a server implementation detail: a settle can apply dozens of actions between
/// two broadcasts, and a view that grew without limit would make the pathological case
/// (a long chain of triggers, an AI-only table racing ahead) the most expensive message
/// the protocol sends, at exactly the moment the receiver is least able to watch it. A
/// client that is behind by more than this has already missed moments and must say so
/// by *catching up*, never by asking for the rest — there is no backfill request in this
/// protocol and there will not be one.
pub const PRESENTATION_WINDOW: usize = 32;

/// One **presentation moment**: a single thing that visibly happened, in the order it
/// happened, carried so a client can *pace* what the board already shows (issue #594).
///
/// **Why this exists at all.** The server applies an action and then settles — resolving
/// the stack, passing priority for idle seats (ADR 0020), advancing steps — before it
/// broadcasts, and the per-seat view channel is latest-value: a view pushed while an
/// earlier one is still in flight replaces it. Both together mean a receiver is handed a
/// *final* board where the game passed through a sequence of causal states, and no amount
/// of client-side diffing recovers the order those states happened in. Diffing two boards
/// says a creature is gone; it cannot say whether it was countered, killed by damage,
/// sacrificed to its own resolution, or exiled and returned. This field carries the
/// server's answer instead of inviting the client to invent one.
///
/// **The same no-loss contract [`log`](crate::GameView::log) uses.** Every view carries
/// the recent unconsumed suffix, bounded by [`PRESENTATION_WINDOW`], so nothing depends
/// on a client having seen the previous message. A reconnecting client, a client whose
/// intermediate view was overtaken, and a client that just joined all read the same
/// field the same way.
///
/// **Advisory, display-only, never load-bearing.** The board, the legal actions, and the
/// result are reconstructable from the view *alone*; a client that drops this field
/// entirely (the CLI and the AI harness do) plays exactly the same game. Nothing here
/// may gate applying a view: a presentation delays a *caption*, never the state. In
/// particular a moment is not a rules event — it does not say an action is legal, that a
/// permanent is still on the battlefield, or that a spell is still on the stack. Those
/// facts live in the view's own fields, always.
///
/// **Identity, and the gaps in it.** [`id`](Self::id) is monotonic per room, which makes
/// it a de-duplication key and a watermark: a client renders ids above the highest it has
/// staged and discards the rest, so an overlapping window costs nothing. A receiver's
/// stream **may have gaps**, for two independent reasons — the window is bounded, and
/// per-seat moments (a [`MomentKind::PhasesSkipped`], which names where *this* receiver
/// was passed) are filtered out of every other seat's stream. A client therefore **must
/// not** treat a missing id as a lost message, wait for it, or synthesize it. Ids are
/// opaque ordering handles; nothing may be derived from their arithmetic beyond "later".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationMoment {
    /// Monotonically increasing per room. The de-duplication key and the ordering
    /// authority — a client stages moments in ascending `id` and never re-orders or
    /// re-sorts them. A receiver's stream may start after `1` and may skip values; see
    /// the type docs on why, and why neither is an error to recover from.
    pub id: u64,
    /// The **causal group** this moment belongs to: one applied action together with the
    /// settle that followed it. Every moment produced by one server step shares a batch
    /// id, so a client can tell "these six things happened because of that one click"
    /// from "these six things are six separate turns of the crank" — a distinction the
    /// timing of arrivals cannot carry, because a whole batch arrives in a single view.
    /// Grouping is the only sanctioned use: a batch is not a transaction, not an undo
    /// unit, and carries no rules meaning.
    pub batch: u64,
    /// The turn number this moment happened on — **not** necessarily the view's current
    /// [`turn`](crate::GameView::turn), which has already moved on by the time a
    /// cross-turn settle is broadcast. A client labels the moment with this and reads
    /// the current position from the view.
    pub turn: u32,
    /// The step this moment happened at, with the same caveat as [`Self::turn`]: it is
    /// where the game *was*, not where it *is*. A client MUST NOT drive a phase plaque
    /// or any current-position UI from this field.
    pub phase: Phase,
    /// What happened — see [`MomentKind`]. A kind a client does not recognize is
    /// **unclassified**, not an error: it is skipped or rendered generically, exactly as
    /// docs/protocol.md requires of every classifying field.
    pub kind: MomentKind,
    /// The [`id`](Self::id) of the moment that **caused** this one, when the server knows
    /// it: the resolution a zone move followed from, the death a graveyard move followed
    /// from. Stated rather than inferred, because adjacency is not causation — a settle
    /// interleaves independent seats' events, so "the moment before" is routinely
    /// unrelated. Omitted when the cause is unknown or the moment is a root; a client
    /// that cannot resolve the referenced id (it fell off the window) simply renders the
    /// moment on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<u64>,
    /// How many identical occurrences this one moment stands for. The server collapses
    /// consecutive moments with an identical [`kind`](Self::kind) — six triggers of the
    /// same ability, four instances of the same damage — into one entry with `count`
    /// raised, so a repeated event costs one caption ("x6") instead of six dwells that
    /// would starve the window of anything worth watching. Always at least `1`; omitted
    /// from the wire at `1`, which is what an older server and every non-aggregated
    /// moment means. It is an occurrence tally, never an amount: damage and life carry
    /// their own magnitudes.
    #[serde(default = "crate::default_one", skip_serializing_if = "crate::is_one")]
    pub count: u32,
}

/// What a [`PresentationMoment`] shows (issue #594).
///
/// The vocabulary is deliberately **narrower than [`GameLogEvent`](crate::GameLogEvent)**
/// and answers a different question. The log is the authoritative record of *what
/// happened*, written to be read as prose after the fact; this is the set of things worth
/// giving a beat of screen time to as they happen. So a mulligan or a kept hand has a log
/// entry and no moment (nothing on the board moves), while a zone move has a moment and
/// no distinct log entry (the log names the resolution that caused it).
///
/// **Counter changes are deliberately absent.** A `+1/+1` counter landing is exactly the
/// kind of small board change this contract exists to give a beat to — but the engine
/// emits no counter-change event and no general zone-change event, and a server that
/// diffed two states to manufacture one would be inventing game information in the very
/// layer whose entire purpose is to stop clients doing that. The honest answer is a
/// missing moment; the dishonest one is a guessed moment. When the engine states counter
/// changes, a variant is added here and both ends learn it in the same PR.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MomentKind {
    /// A player put a spell on the stack (CR 601.2).
    Cast {
        /// The caster.
        player: PlayerId,
        /// The cast object, retained — see [`MomentObject`].
        object: MomentObject,
    },
    /// A spell or ability finished resolving (CR 608.2), neither countered nor fizzled.
    Resolved {
        /// The controller of the resolving object.
        player: PlayerId,
        /// The object that resolved, retained.
        object: MomentObject,
    },
    /// A spell or ability was countered (CR 701.5) and left the stack without resolving.
    Countered {
        /// The countered object's controller.
        player: PlayerId,
        /// The countered object, retained.
        object: MomentObject,
    },
    /// A spell or ability was removed from the stack because **all** of its targets had
    /// become illegal (CR 608.2b) — the "fizzle". Distinct from [`Self::Countered`]: no
    /// effect countered it, so a client that conflates the two would credit an opponent
    /// with an answer they never had.
    Fizzled {
        /// The fizzled object's controller.
        player: PlayerId,
        /// The fizzled object, retained.
        object: MomentObject,
    },
    /// An object changed zones (CR 400.7): the travel a client animates. Both endpoints
    /// are stated, because the destination alone cannot say what kind of movement this
    /// was, and the client is not permitted to work it out from a board diff.
    ZoneMove {
        /// The object that moved, retained — it is by definition no longer where it was.
        object: MomentObject,
        /// The zone it left.
        from: MomentZone,
        /// The zone it arrived in.
        to: MomentZone,
    },
    /// A creature **died** — moved from the battlefield to a graveyard (CR 700.4). Only
    /// creatures die; any other permanent reaching a graveyard is a [`Self::ZoneMove`]
    /// and nothing more, the same line [`GameLogEvent::PermanentDied`](crate::GameLogEvent::PermanentDied)
    /// draws.
    Died {
        /// The permanent that died, retained.
        object: MomentObject,
    },
    /// Damage was dealt to a player or marked on a permanent (CR 119), lethal or not.
    Damage {
        /// What took the damage — the same tagged target the log uses.
        target: LogDamageTarget,
        /// How much.
        amount: u32,
    },
    /// A life total changed by this signed amount from a **non-damage** source (CR 118):
    /// life gained, paid, or lost. Damage is [`Self::Damage`], so the two never
    /// double-report one hit.
    Life {
        /// The affected player.
        player: PlayerId,
        /// Signed life-total delta.
        amount: i32,
    },
    /// A player declared attackers (CR 508.1). Carried as the whole declaration, not one
    /// moment per attacker: the beat a player watches is "three creatures attack", and
    /// splitting it would spend the window on a list.
    Attacked {
        /// The attacking player.
        player: PlayerId,
        /// The declared attackers, retained, in declaration order. Possibly empty — a
        /// declaration of no attackers is still a declaration.
        attackers: Vec<MomentObject>,
    },
    /// A player declared blocker-to-attacker assignments (CR 509.1), carried whole for
    /// the same reason [`Self::Attacked`] is.
    Blocked {
        /// The defending player.
        player: PlayerId,
        /// The assignments, as the same pairs the log carries.
        blocks: Vec<LogBlock>,
    },
    /// A player drew cards (CR 121.1). Identities are **absent by construction**, exactly
    /// as in the log: a draw moment is a count and a player, and no redaction pass is
    /// needed to keep it that way.
    Drew {
        /// The player who drew.
        player: PlayerId,
        /// How many cards.
        count: u32,
    },
    /// The turn passed to a new turn number and/or a new active player (CR 500.1). Emitted
    /// in addition to the [`Self::PhaseChange`] for the step that opened the turn, because
    /// a turn boundary is the one pacing beat worth its own dwell.
    TurnChange {
        /// The new turn number.
        turn: u32,
        /// The player whose turn it now is.
        active_player: PlayerId,
    },
    /// The game entered a new step (CR 500.1). The cheapest moment in the vocabulary and
    /// the first a client should drop when it is behind — a phase is a caption, and the
    /// view's own [`phase`](crate::GameView::phase) is always current regardless.
    PhaseChange {
        /// The step entered.
        phase: Phase,
    },
    /// The room **passed priority for this receiver** at these positions during the
    /// settle (issue #455, ADR 0020, CR 117.3) — the moment form of
    /// [`GameView::auto_passed_steps`](crate::GameView::auto_passed_steps), so the "we
    /// moved without you" beat takes its place in the same ordered stream as the events
    /// it happened around.
    ///
    /// **Per-seat, and the sole reason a receiver's id stream has gaps.** It names
    /// positions where *this* receiver was passed; another seat's is that seat's moment,
    /// filtered out of this stream. Spectators receive none at all — there is no seat to
    /// speak for.
    ///
    /// The whole path is **one moment**, never one per priority window: several passes
    /// inside a settle read as one skipped stretch, and the ordered path is exactly what
    /// [`AutoPassedStep`] already carries (turn included on every entry, because a
    /// repeated step means an extra combat phase (CR 506.1) or an extra cleanup
    /// (CR 514.3a) at least as often as it means a new turn).
    PhasesSkipped {
        /// The ordered path of positions passed, oldest first.
        steps: Vec<AutoPassedStep>,
        /// Why the room acted — see [`AutoPassReason`].
        reason: AutoPassReason,
    },
    /// A player left the game mid-game under CR 800.4a — they lost while two or more
    /// players remained, so play continues without them. Distinct from
    /// [`Self::GameOver`], which fires only once one player is left; a two-player loss
    /// produces `GameOver` alone.
    Eliminated {
        /// The player who left the game.
        player: PlayerId,
        /// Why they lost (CR 104.3 / 704.5).
        reason: GameOverReason,
    },
    /// The game ended (CR 104.2a). The last moment a room produces. It repeats what
    /// [`GameView::result`](crate::GameView::result) already states — the view remains
    /// the authority a client renders the verdict from; this only gives the ending its
    /// place in the ordered stream.
    GameOver {
        /// The terminal result.
        result: GameResult,
    },
}

/// The **retained snapshot** of an object a moment refers to (issue #594).
///
/// A moment is shown *after* the state it describes is gone: the creature that died is
/// not on the battlefield, the countered spell is not on the stack, and the view carrying
/// the moment is the one where they are already absent. A reference by id alone would
/// therefore resolve to nothing at exactly the moment it is needed, which is why the name
/// — and, when the room had one, the face — travel with the moment instead of being
/// looked up.
///
/// **Fixed at record time, never re-resolved.** The name is what the object was called
/// when the moment was recorded, the same promise
/// [`LogEntity::name`](crate::LogEntity::name) makes; a later copy effect, a name change,
/// or a new object reusing the id does not retroactively edit history.
///
/// **Public faces only.** [`card`](Self::card) is present only when the object was
/// visible in a public zone — battlefield, stack, graveyard, exile, command — at the time
/// the server observed it. A hand or library face is **never** retained, for any receiver,
/// under any aggregation: this type crosses seats (a spectator and every opponent read the
/// same public moments), so a retained private face would be an information leak with no
/// way to redact it after the fact. `None` means "no public face was known", never "hidden
/// from you specifically" — a client renders the name and stops, and MUST NOT infer
/// secrecy, zone, or ownership from the absence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MomentObject {
    /// The object's opaque entity id — a presentational handle for highlighting only,
    /// which may already refer to nothing on the current board.
    pub id: EntityId,
    /// The display name, fixed when the moment was recorded.
    pub name: String,
    /// The retained **public** face, when one was known. Omitted otherwise; see the type
    /// docs for why its absence carries no information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<CardView>,
}

/// A zone endpoint of a [`MomentKind::ZoneMove`] (issue #594).
///
/// A closed, presentational vocabulary: it names *where a client should animate from and
/// to*, not the engine's zone model. Naming both endpoints is what keeps the client out
/// of the rules — "graveyard → battlefield" is a reanimation and "hand → battlefield" is
/// a land drop, and neither is recoverable from a board diff that shows only the arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MomentZone {
    /// The battlefield (CR 403).
    Battlefield,
    /// A graveyard (CR 404).
    Graveyard,
    /// The exile zone (CR 406).
    Exile,
    /// A hand (CR 402) — an endpoint only; a moment never retains a hand *face*.
    Hand,
    /// A library (CR 401) — likewise an endpoint only.
    Library,
    /// The stack (CR 405).
    Stack,
    /// The command zone (CR 408).
    Command,
}

/// Why the room passed priority for a seat in a [`MomentKind::PhasesSkipped`]
/// (issue #594, ADR 0020).
///
/// Two reasons, because they are two different sentences to a player, and a client that
/// had to choose one caption for both would have to guess which. The judgment is the
/// server's alone — as with everything about automation, a client renders the stated
/// reason and never derives it from the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoPassReason {
    /// The seat had **no meaningful action** at those positions (CR 117.3 priority with
    /// nothing to do), and had not asked to stop there — the ordinary ADR 0020 pass.
    NoResponseAvailable,
    /// The seat faced a **declaration with no legal non-empty answer** (issue #453): a
    /// `declare_attackers` with no eligible attacker, a `declare_blockers` with no
    /// eligible blocker. The room submitted the empty declaration rather than prompting
    /// for a choice that did not exist. Never used for a declaration the seat could have
    /// answered non-emptily — automation resolves no real choice.
    ForcedDeclaration,
}

#[cfg(test)]
mod tests;
