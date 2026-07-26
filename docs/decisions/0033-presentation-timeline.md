# ADR 0033: Presentation timeline — an authoritative moment window on every view

- Status: accepted
- Date: 2026-07-26
- Issue: #594

## Context

ADR 0020 bought pace by letting the room act for an idle seat, and #455/#593 refined
*when* it may. Neither addresses the other half of the same complaint:

> Auto-pass may skip unnecessary **input**. It must not skip observable **cause and
> effect**.

A seat with no legal response should not be asked to acknowledge a removal spell. It
should still see the spell go on the stack, resolve, and kill something, in that order.
Today it sees none of that, and the reason is transport, not rendering. The room applies
an action and then *settles* — resolving, auto-passing, advancing steps — before it
broadcasts, and the per-seat outbox is a latest-value `watch`: a view pushed while an
earlier one is unread replaces it. The receiver is handed the **final** board of a
sequence that never reached it.

No client-side reconstruction recovers what was lost. A diff of two views says a creature
is gone; it cannot say whether it was countered, burned, sacrificed to its own resolution,
or exiled. Deriving that would be the client asserting game structure the server never
stated — the thing ADR 0020's `auto_passed_steps` follow-up already refused to allow.
ADR 0021's log preserves the *facts* after the fact, but a history panel is not a live
sequence, and its entries carry no notion of what still needs showing.

## Decision

**The server states the order; the client only schedules it.** Every `GameView` and
`SpectatorView` carries `presentation`: an ordered, bounded, monotonically identified
window of **presentation moments** — what visibly happened on the way to this state.

- **The window rides every view, exactly as ADR 0021's log window does.** #594 permits
  three no-loss contracts; this is the first — "include the recent unconsumed authoritative
  suffix in the newest view". It is the only one compatible with the invariant that a
  fresh view is sufficient to rebuild the UI. An **event-stream protocol** — a second
  message class carrying deltas — is rejected on those grounds: it would make a receiver's
  screen depend on messages it did not receive, and the latest-value channel would coalesce
  the stream anyway. A **cursor/acknowledgment** mechanism is rejected because it would put
  per-seat *delivery* state in a room that deliberately holds none: the room holds seat
  preferences and names, re-sends full state on reconnect, and never waits on a client.
- **Moments are derived, never a second source of truth.** The public moments are a pure
  mapping of the suffix of the existing receiver-safe protocol log past a cursor; the
  per-seat grouped `phases_skipped` moment is a projection of the room's existing
  `auto_passed_steps` accumulator (#455). There is no new emission seam in the engine and
  nothing to keep in sync — a moment cannot contradict the log or the board because it is
  computed from them. What cannot be derived stays **absent**: the engine emits no
  counter-change event, so there are no counter moments, rather than faked ones.
- **Retained faces come from public zones only.** A moment must render an object the final
  view no longer contains, so the room caches the faces it observes before each applied
  action — battlefield, stack, graveyard, exile, command. It **never** caches a hand or
  library face. That is the whole information-leak guard: `presentation` is the one field
  that outlives its object, so a hidden face cached here would leak past the redaction that
  every other field respects.
- **The server never sleeps and gameplay is never buffered.** Dwell is entirely
  client-side, computed by a pure scheduler over an injected clock (ADR 0030's client, one
  more display-only layer). `store.view` is applied the moment it arrives, always; a client
  may delay a *caption*, never a board, an action set, or a decision timer. CLI, headless,
  and AI consumers ignore the field and play the identical game, `AutoPassPolicy::Off` is
  unchanged in behavior (moments still ride; `auto_passed_steps` stays empty, so no
  `phases_skipped` is ever produced), and nothing blocking enters any hot loop.

## Consequences

**Bounded means lossy, deliberately.** A client stalled longer than the window — a
throttled tab, a long stall, a reconnect — loses old moments and must fast-forward to the
newest id and render the current board. That is the wanted behavior (#594 prefers a concise
catch-up to replaying a stale turn in real time), but it makes gaps *normal*, not
exceptional: per-seat moments are filtered out of other seats' streams too. A client must
de-duplicate by id, never fill a gap, wait, re-sort, or request backfill — there is no
backfill request in this protocol. Widening the window is a contract change.

**The timeline's vocabulary is the log's vocabulary.** Deriving from ADR 0021 is what makes
moments trustworthy and what caps them: a new moment kind needs a log event at the right
seam first. Counter changes, and any general zone change the engine does not log, wait on
that.

**The client scheduler is display-only, and stays that way.** It computes no legality,
submits no passes, and cannot delay applying a view; reduced motion drops travel and tween
but keeps the ordered captions, because a dwell is information, not animation. If the
scheduler is deleted tomorrow, the game is still correct — only less legible, which is the
condition #455 reported in the first place.
