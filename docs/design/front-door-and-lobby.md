# The front door and lobby — the pregame and postgame experience

**The design authority for everything around the match** (issue #461, under
[ADR 0029](../decisions/0029-2-5d-presentation-direction.md) /
[ADR 0030](../decisions/0030-2-5d-presentation-architecture.md), master issue
#464 Phase 4). It decides the end-to-end flow — home → lobby → room → match →
back — including reconnect, spectating, and the postgame landing, and it fixes
the composition that [#506](https://github.com/ninthworld/rune/issues/506)
implements. It is a **presentation** decision: no protocol change.

Relationships to the other authorities:

- [`visual-system.md`](visual-system.md) is binding. This document applies its
  §2 color system, §3 elevation, §6 identity, §7 non-color state channels, and
  §8 motion grammar to the screen-space surfaces **outside** the battlefield
  plane. Where the two disagree, the visual system wins.
- [`layout-model.md`](layout-model.md) owns the plane only. Nothing here stages
  on the plane; the one thing carried across is ADR 0023's **one action home**
  commitment, restated for pregame in §4.4.
- [`presentation-budgets.md`](presentation-budgets.md) caps everything: 44 px
  targets, 4.5:1 text / 3:1 indicator contrast, 125 % text scaling, the motion
  classes, and the ≤ 4 MB first-match / ≤ 5 s cold-start load ceilings that
  bound any front-door backdrop (§7).
- [`ui-requirements.md`](ui-requirements.md) stays binding — this styles
  capabilities, it never removes one.
- [ADR 0012](../decisions/0012-lobby-protocol.md) (lobby protocol),
  [ADR 0022](../decisions/0022-spectator-view-model.md) (spectators), and
  [ADR 0027](../decisions/0027-deck-persistence.md) (device-local decks) are
  **untouched**. Every affordance below is derived from the shipped
  `LobbyView.valid_commands` or is a client-session action.
- [`ui-design-notes.md`](ui-design-notes.md) §Front door remains the record of
  the shipped screens. §5 below supersedes its *composition*; its behavioral
  commitments (one gold CTA, choices as visible presses, glyph-coded state,
  never a dead screen) are carried forward verbatim.

## 1. The shipped flow, walked

Line references are `clients/web/src` unless noted, at `main` as of this
document.

### 1.1 Front door

`ConnectionScreen.tsx` renders a centered carved panel: brand lockup, a status
pill, one gold **Play**, a "Server settings" disclosure, and a **Display
settings** button (`ConnectionScreen.tsx:185-202`). Three states — `idle`,
`connecting`, `closed` — each keeping an interactive control on screen
(`:134-156`, `:162-207`). This screen is the healthiest surface in the flow.

### 1.2 Entering the lobby

The socket opens, `App` switches on `status === 'open'` (`App.tsx:61-63`), and
`LobbyScreen` renders `LobbyWaiting` until the first `LobbyView` arrives —
a header, a status line, and Disconnect (`LobbyScreen.tsx:258-273`).

### 1.3 Room-less lobby

One vertical column inside a carved shell: header (brand + Disconnect,
`:159-177`), the "Playing as" identity strip (`:187-255`), then three peer
sections — the room directory, Create a room, Join a friend
(`RoomEntry`, `:530-543`).

### 1.4 In a room

The same column, now: room header with the setup name, a `filled/total · ready`
line, a copyable room-id chip, and Leave room (`:850-883`); the seat roster
(`:885-896`); optional host-only AI seating (`:905-921`); the starter-deck grid
plus **Build a deck** (`:923-961`); and, last, the CTA area where exactly one
control is gold — Submit deck until decked, then Ready (`:984-1037`).

### 1.5 Into the match

Every seat filled + decked + ready, the server constructs the game, the first
`GameView` arrives, and `App` swaps the whole screen for `LiveMatchTable`
(`App.tsx:51-52`).

### 1.6 Postgame

A terminal `GameView` renders `GameOverOverlay` — headline, winner, reason, and
no controls (`table/GameOverOverlay.tsx:82-107`).

### 1.7 Spectating

A directory row for an `in_progress` room offers **Spectate** when the server
advertises `spectate_room` (`LobbyScreen.tsx:302-317`). The spectator frame
replaces everything (`store.ts:510`) and `SpectatorTable` mounts.

### 1.8 Reconnect

`restoreSession()` runs once on mount, including a hard reload
(`App.tsx:45-47`), replaying the stored session token so the server reunites the
connection with its held seat (ADR 0012).

### The problems

| # | Problem | Evidence |
| --- | --- | --- |
| P1 | **Two visual languages meet at the ready gate.** Pregame is carved panels (line-work borders with corner notches) on a chrome vignette; the match is the scene environment built from `SCENE_THEMES`. A single lobby screen already mixes both — only the deck grid crossed over. | `screens.module.css:17-33` (`.screen` vignette), `:49-80` (carved panels); `table/live/LivePlane.tsx:61-66` and `table/live/live-plane.module.css:23-103` (environment); `LobbyScreen.tsx:937` (`deckSceneVars`) |
| P2 | **The same seat is a different color before and after the gate.** The roster colors seat 0 teal `#3E9C9C`; the match colors seat 0 azure `#4D7EC9`. "Who am I" is taught twice, differently, three seconds apart. | `LobbyScreen.tsx:71,148-152` → `table/identityAccents.ts:22-31`; `table/live/gameViewPresentation.ts:93-96` → `sceneTokens.ts:137-144` |
| P3 | **The one advance-the-game control is below everything.** In a room the column runs roster → AI seating → five deck tiles → Build a deck → CTA. At a filled four-seat room the single gold control is off-screen at the 1280×800 desktop floor, and the roster that explains *why* it is disabled-looking is at the top. | `LobbyScreen.tsx:847-1037` |
| P4 | **The gate is never drawn.** Deck state is a chip in the roster; the deck picker is a separate section; the CTA silently relabels from Submit deck to Ready. Nothing connects "you are not ready" to "your deck is not submitted". | `LobbyScreen.tsx:621-642` (chips), `:923-961` (picker), `:993-1021` (relabel) |
| P5 | **The primary path degrades into a scroll hunt.** Directory, Create, and Join are three stacked peers; the empty directory points "below". | `LobbyScreen.tsx:376-379`, `:530-543` |
| P6 | **Inviting someone is an opaque id.** The invite is a raw room id in a copy chip, redeemed by pasting into a text field whose placeholder is the only instruction. There is no link and no explanation. | `LobbyScreen.tsx:858-871`, `:496-514` |
| P7 | **Settings vanish the moment you connect.** The #505 surface is on the front door only; the in-match `GameMenu` renders only when `onChoose && onShowShortcuts` are passed, so quality/motion/art settings are unreachable across the entire lobby, room, spectate, and game-over span. | `ConnectionScreen.tsx:195-202`; `table/TopBar.tsx:102-106`; `table/GameMenu.tsx:26-27,93-106` |
| P8 | **The pre-first-frame state is a sentence.** No skeleton, no sense of what is arriving. | `LobbyScreen.tsx:258-273` |
| P9 | **Postgame is a dead end.** No exit control exists anywhere on the live path, and a lobby frame never clears `view`, so `App` stays pinned to the terminal table. | `table/GameOverOverlay.tsx:82-107`; `store.ts:536-558`; `App.tsx:51-52` (#452) |
| P10 | **Spectating is the same trap.** Entry exists; the spectator frame clears `lobby`; `SpectatorTable` mounts `TopBar` without menu handlers, so there is no stop-watching control and no settings — even though the server already accepts `Leave` from a spectator and re-projects a room-less `LobbyView`. | `LobbyScreen.tsx:302-317`; `store.ts:510`; `table/SpectatorTable.tsx:173`; `crates/rune-server/src/lobby/commands.rs:512-524` |
| P11 | **Reconnect is invisible.** A returning player sees the generic "Opening a connection to …" and, on arrival, no cue that a held seat was reclaimed. | `App.tsx:45-47`; `ConnectionScreen.tsx:134-156` |
| P12 | **There is no place the player *is*.** Home, lobby, and room are three full-page mounts of the same `.screen` with nothing continuous between them, and nothing continuous with the match. | `screens.module.css:22-33`; `App.tsx:51-64` |
| P13 | **A match begins with no warning.** Ready is a per-seat chip; the instant the gate closes the screen is replaced. | `App.tsx:51-52` |

P1–P8 and P12–P13 are #506's to fix. P9 is #452's fix landing in the destination
this document defines. P10 is a follow-up (§9). P11 is split: the front-door
copy is #506's, the arrival cue is #509's.

## 2. The intended flow

**Four places on one continuous stage.** Every place is derived from store
state, so the flow is reconstructable from one message plus the socket status —
nothing is load-bearing across messages.

| Place | Reached when | What it is for | Exits |
| --- | --- | --- | --- |
| **Front door** | no `view`, no `spectatorView`, no `lobby`, socket not open | connect, settings, a reconnect notice | Play → Lobby |
| **Lobby** | socket open, `lobby.room === undefined` | find a game or start one | Join/Create → Room; Spectate → Watching; Disconnect → Front door |
| **Room** | `lobby.room !== undefined` | roster, deck, the ready gate | Leave → Lobby; gate met → Match |
| **Match** | `view !== null` | play; and at the end, the verdict | exit at game over → Lobby (landing) |
| **Watching** | `spectatorView !== null` | read-only match | Stop watching → Lobby |

Rules that hold in every place:

1. **Never a dead screen** — every place has at least one working exit and one
   working settings door, in every state including error and pre-first-frame.
2. **One gold** — exactly one advance-the-game affordance is gold at a time
   (carried from the shipped lobby).
3. **`valid_commands` is the only source of interactivity** — client-session
   actions (connect, disconnect, open settings, dismiss the verdict) are the
   only controls that do not come from the view.
4. **The stage persists** — the environment backdrop is mounted once and never
   re-mounts across a place change (§5.0). Place changes move content, not the
   world.

### The sequence

- **Home → Lobby.** Play connects. The front door's content column recedes and
  the lobby's rises on the same backdrop; the brand lockup shrinks in place into
  the lobby header rather than disappearing and reappearing.
- **Lobby → Room.** Joining a directory row, creating a room, or being restored
  into one all land on the same Room composition. There is no separate
  "creating…" screen: the next `LobbyView` carries the room.
- **Room → Match.** The gate closes server-side; the first `GameView` arrives.
  The pregame column drops away inside #509's game-start window (§5.4). The
  environment behind it is already the match's environment, so the crossing has
  no style boundary — the panels leave and the table assembles on the same sky.
- **Match → Lobby (postgame).** The verdict is part of the terminal `GameView`
  and stays in the match (§5.5). Its exit **closes the socket and reopens the
  same server**, clearing `view` (#452's store transition); the scene recedes
  per visual-system §8 "Return to lobby" and the Lobby place rises with a
  **last-match ribbon**.

  > **Correction (#452/#506, as shipped).** An earlier draft of this document
  > said the exit "sends `leave` (already advertised)". That is not possible on
  > a match connection: once a socket is bridged to a room, `serve_connection`
  > routes every decoded message to the room as `RoomInput::Message`
  > (`crates/rune-server/src/room/connection.rs:18-24`), so the lobby never sees
  > a `leave`. Closing the socket *is* how the seat leaves — the bridge sends
  > `RoomInput::Leave` on exit — and the client then reopens the same server for
  > a fresh `Hello` and its own first `LobbyView`. Still **no protocol change**;
  > the landing is simply reached across a reconnect, so nothing in the
  > transition may assume a same-session hand-off. (§5.6's spectator exit is a
  > genuine `leave` and is unaffected: a spectator connection is not bridged.)
- **Reconnect.** `restoreSession()` on mount puts the front door in a
  *Reclaiming your seat* state rather than the generic connecting state. Where
  the reclaimed session lands — front door, lobby, room, match, or watching — is
  entirely the server's answer; the client simply renders the place the returned
  view names. The in-match arrival cue is visual-system §8's
  reconnect/fast-forward pulse (#509).
- **Spectating.** Spectate from a directory row → Watching. Stop watching sends
  `leave`, which the server already answers with a room-less `LobbyView`
  (`lobby/commands.rs:512-524`), landing back in Lobby. No protocol change.

## 3. Alternatives considered

**A. Restyle the existing panels in place.** Swap the carved-panel CSS for scene
tokens, change nothing structural. *Rejected*: it fixes P1/P2 and leaves
P3–P6 and P12–P13 exactly as they are, and #464 workstream 6 explicitly requires
these screens get their own design work rather than a literal inheritance of the
battlefield baseline.

**B. A linear wizard** — name → game type → deck → ready, one step per screen.
*Rejected*: it fights ADR 0012. A wizard has a cursor, and a cursor is client
state that must survive `LobbyView` frames it does not control (the directory
changes under you; seats fill in any order; a seat can un-ready at any time).
The shipped "everything derived from one view" discipline is the right one.

**C. Stage the lobby on the battlefield plane** — an actual table you sit at,
seats arranged around it, filling as players join. *Rejected for v1* on two
grounds: it would need plane geometry, crest clusters, and staging for a state
that has no game, and it would imply a seat arrangement that the game has not
chosen yet. The room roster keeps seat identity (§5.3) without pretending to be
a board. Recorded as a possible later flourish, not a requirement.

**D. A persistent pregame stage: one environment backdrop, screen-space content
that changes.** **Chosen.** It is the smallest thing that makes the crossing
into the match invisible, costs no asset bytes (§5.0), and leaves the pregame
composition free to be a good *screen* rather than a pretend table.

**E. The postgame landing.** Two candidates:

- *A dedicated postgame/results screen* between the match and the lobby, with
  the result, a summary, and Play again. Rejected: the result lives on the
  terminal `GameView`, which the server re-sends on reconnect into a finished
  game (#452's acceptance criterion). Moving the verdict off the match means
  either carrying it as load-bearing client state across the transition, or
  losing it on reload — and a whole screen whose only content is one sentence.
- *Verdict in the match, landing in the lobby.* **Chosen.** The verdict is
  rendered where its data lives, exactly as today, so it survives reconnect for
  free; the landing is the Lobby place the player already understands, plus a
  quiet, explicitly-ephemeral last-match ribbon (§5.5).

**F. Rematch.** *Rejected as out of reach without a protocol change.* A finished
room is reclaimed by the registry once its game task stops
(`crates/rune-server/src/lobby/registry.rs:44-58`), and there is no command that
returns a started room to `gathering`. "Play again" therefore honestly means
*create a new room with the same configuration* (§5.5). A true rematch is a
follow-up (§9) and rides the usual contract rules.

## 4. The chosen direction

1. **One stage, four places.** The environment is mounted once by a pregame
   stage shell; Home, Lobby, and Room are content compositions inside it.
2. **The environment is the continuity, and it is free.** The match's backdrop
   is pure CSS gradients over `SCENE_THEMES` values — three parallax groups with
   no image assets (`table/live/live-plane.module.css:23-103`). The front door
   reuses the same recipe and therefore adds **zero bytes** to the ≤ 4 MB
   first-match download and nothing to the ≤ 5 s cold start.
3. **Seat identity is taught once.** Pregame and match read
   `SCENE_SEAT_ACCENTS` from the same index, so a seat's color never changes as
   the game starts.
4. **One gold, always on screen.** The room's advance-the-game control lives in
   a persistent bottom **ready bar**, not at the end of a scroll — the pregame
   echo of ADR 0023's one action home.
5. **The verdict belongs to the match; the landing belongs to the lobby.**
6. **Every place has an exit and a settings door**, including Watching and game
   over.

## 5. Composition

### 5.0 The shared stage

A `PregameStage` shell owns three layers, bottom to top:

| Layer | Content | Source |
| --- | --- | --- |
| Environment | sky gradient + two ambient glows, far-ground silhouettes, arena edge | the `.environment / .sky / .farGround / .arenaEdge` recipe of `live-plane.module.css`, fed by `SCENE_THEMES[DEFAULT_SCENE_THEME]` |
| Content | the place's panels and columns, in screen space | §5.1–5.3 |
| Overlays | settings, deck builder, confirmations | `SCENE_ELEVATION.screen` |

Tokens reach CSS through a `pregameSceneVars(reducedMotion)` builder in the
`deck/deckScene.ts` mold (`deckScene.ts:35-55`) — the ADR 0019 pattern: scene
values assembled in TS, consumed as `--pregame-*` custom properties. It must
publish, at minimum, the `SCENE_NEUTRALS` surfaces, `SCENE_HUES.gold/blue`, the
`SCENE_ELEVATION` shadows, the theme's six environment slots, and
`sceneMotionMs('micro' | 'staging', reducedMotion)` with their easings. No new
token values: if a value is needed and absent, it is added to `sceneTokens.ts`
under its lockstep test, not invented in CSS.

**Surfaces.** Panels are `SCENE_NEUTRALS.raised` (`#23262B`) with
`lineFaint`/`lineStrong` bounds; text is `SCENE_NEUTRALS.text`. The carved
corner-notch treatment of `screens.module.css:49-80` is retired — depth comes
from the elevation ladder, not from drawn ornament (visual-system §1.1).

**Elevation.** Panels rest at `SCENE_ELEVATION.screen`. Interactive tiles and
rows (directory rows, deck tiles, choice tiles, segments) sit at `rest`, lift to
`lifted` on hover/keyboard focus, and go to `held` while selected or pressed —
the same ladder the deck grid already uses.

**Quality tier.** The stage carries the same `data-environment="on | reduced |
off"` attribute the match's environment uses, driven by the shared
`presentationSettings` store: ambient drift on at High, slowed at Standard, off
at Lite (a static gradient). The content layer is never degraded.

**Reduced motion.** Every duration resolves through `sceneMotionMs(...)`, so
reduced motion is a token-level `0`; ambient drift stops; no place change,
lift, or ribbon animates. Layout and state are identical either way.

### 5.1 Front door

A centered column, ~480 px wide, on the stage.

- **Brand lockup** — `RuneMark` + display-face wordmark + tagline, unchanged
  procedural geometry (no images, no frames, no WotC marks).
- **Status pill** — idle / connecting / disconnected, keeping today's three
  visually distinct states. Non-color channels stay: a live pulse for
  connecting (static under reduced motion), a word for each state.
- **One gold Play** (Retry when closed), the only gold on the screen.
- **Server settings** disclosure, unchanged behavior including auto-open on a
  closed connection.
- **Settings** — the #505 entry point, see §6.
- **Reconnect state (P11)** — when `restoreSession()` has a stored session for
  this address, the connecting state reads *Reclaiming your seat* instead of
  *Opening a connection*, and the pill keeps the connecting treatment. Purely a
  copy/state change; the socket lifecycle is untouched.

### 5.2 Lobby

Two columns at ≥ 1180 px, one column below (and on phone portrait):

- **Header bar** — compact lockup left; right, the **session menu** (§6) and
  Disconnect. Disconnect stops being the only thing in the header.
- **Identity strip** — the "Playing as <name>" row with its inline editor,
  unchanged, wearing the local player's crest chip (§5.3) so the name and the
  color are learned together.
- **Left / primary — Open games.** The directory, ahead of everything. Each row
  is a lift-on-focus tile: setup label, `filled/total` with a **seat-pip row**
  (one pip per seat, filled pips in that seat's `SCENE_SEAT_ACCENTS` color, empty
  pips as dashed outlines) so occupancy reads as a shape, not only a number;
  spectator count when > 0; and the row's action — Join, Full, or In progress +
  Spectate, exactly as advertised by `valid_commands`. The empty state is a
  full-width invitation that *contains* the Create action rather than pointing
  at it (fixes P5).
- **Right / secondary — Start a game.** Create and Join-by-id become one card
  with two visible modes (a segmented Create / Join switch), so the two
  secondary paths stop competing as peers. Create keeps game-type choice tiles
  and the segmented seat picker; both stay ≥ 44 px and wrap at 125 % text.
- **Sharing (P6).** The room-id chip in the Room keeps Copy and gains one line
  of instruction ("Send this id to a friend — they paste it under Join"), and
  the Join field's label matches that wording. No link scheme is invented: the
  protocol's join key is a room id, and the UI says so plainly.

### 5.3 Room

Two columns at ≥ 1180 px, one below, with a persistent bottom **ready bar**.

- **Room header** — setup name, the live `filled/total seats · N ready` line,
  the room-id chip with Copy plus the share line, and Leave room, kept visually
  apart from the gold.
- **Left — the roster.** One row per seat, each wearing
  `SCENE_SEAT_ACCENTS[seat.seat % 6]` as an edge stripe and a **crest chip**:
  the visual-system §6 crest cluster reduced to chip scale — a monogram (first
  glyph of the display name, or the seat glyph when unnamed) inside a
  seat-accent ring, ≥ 44 px. The local player's chip carries the same ring the
  match's crest wears, which is what makes "that's me" survive the ready gate.
  Deck and ready stay glyph + word chips; open seats stay dashed invitations;
  AI seats keep their tag and the host-only Remove.
- **Right — the deck.** The starter tiles (already scene-token dressed) and
  **Build a deck**, plus, when the format requires one, the designated-commander
  line. Saved decks stay inside the builder (ADR 0027, unchanged).
- **Ready bar (fixes P3, P4).** Pinned to the bottom of the room composition at
  `SCENE_ELEVATION.screen`, always visible at every scroll position and
  geometry. It carries, left to right: the **gate state in words** — *Choose and
  submit a deck* → *Waiting for 2 more players* → *You're ready — waiting for
  Bob* — and the single gold control (Submit deck → Ready), with the quiet Not
  ready fallback beside it once ready. Every string is read from the current
  `LobbyView`; the bar computes no legality and offers only advertised commands.
  This is where P4's missing causality gets drawn: the sentence names the reason
  the gold control is what it is.
- **AI seating** stays host-only and advertised-gated, moved under the roster
  (it is a roster operation, not a deck operation).

### 5.4 Handing off to the match

At the first `GameView`, the pregame content column dims and drops away
(`staging` class, ≤ 400 ms; reduced motion cuts) while the environment stays
mounted and #509's game-start moment assembles the table on it. The handoff
must fit *inside* game start's ≤ 800 ms skippable window rather than extend it,
must never gate input, and must be discarded outright if a newer view arrives
(visual-system §8 interruptibility). Anticipation for P13 is cheap and lives in
the ready bar: once the last seat readies, the bar's sentence becomes *Starting
the game…* on the same frame the view says so — no invented countdown.

### 5.5 The postgame landing

**Beat one — the verdict, in the match.** `GameOverOverlay` stays where it is,
rendered from the terminal `GameView`, so a reconnect into a finished game shows
the identical screen. #452 gives it the exit control and #509 stages it. It
gains one thing from this document: the exit is the *only* gold on that overlay,
and the overlay also exposes the session menu (§6) so settings are reachable at
game over (P7).

**Beat two — the landing, in the lobby.** The exit closes the socket — which is
how a bridged match connection gives up its seat (see the correction in §2) —
clears `view`, and reopens the same server. The scene recedes per visual-system
§8 "Return to lobby" (≤ 400 ms; reduced motion cuts) and the **Lobby** place
rises on the reopened session's first `LobbyView`. Because the landing is on the
far side of a reconnect, the environment is re-mounted rather than carried, and
the ribbon is the *only* thing that crosses; the front door's connecting state
is passed through and says so ("Returning to the lobby…"). On arrival the lobby
shows a **last-match ribbon** above the directory:

- one line — outcome word in the matching §2 hue family (Victory in the gold
  family, Defeat in the red *loss moment* family, Draw neutral) plus the
  opponents' names and the setup label, with the outcome **word** carrying the
  meaning so it is never color-only (§7 rule);
- one action — **Play again**, which opens the Start-a-game card pre-filled with
  the finished room's `game_setup` and seat count and moves focus there. It is
  honestly a *new room*: the finished room has been reclaimed server-side, and
  nothing here pretends otherwise;
- one dismissal — the ribbon closes on any of: Play again, joining or creating
  any room, a reload, or an explicit dismiss.

The ribbon is **presentation-only ephemeral state**, written to the store on the
same transition that clears `view`, in the exact idiom the store already uses
for `lobbyError`: "ephemeral, never load-bearing — the lobby UI still rebuilds
from `lobby` alone" (`store.ts:190-192`, `:524-533`, `:546-549`). A reload loses
the ribbon and loses nothing else. This is the boundary that keeps the AGENTS.md
reconstruct-from-one-view rule intact: the ribbon may never be the only place a
piece of information exists, and no control's availability may depend on it.

**Degenerate cases.** If the socket is closed when the player exits (the server
went away), the exit routes to the **Front door** in its disconnected state,
with the ribbon suppressed — never a dead screen. If the player concedes, the
path is identical; concede is a normal `valid_actions` entry and gets no special
casing.

### 5.6 Watching

The spectate composition is the match's, read-only (#504 already stages it on
the plane). From this document it needs two things, both client-session actions
with no protocol change:

- **Stop watching** — sends `leave`; the server drops the connection from the
  room's spectator roster and re-projects a room-less `LobbyView`
  (`lobby/commands.rs:512-524`), landing in Lobby with the same recede
  transition as the postgame return.
- **The session menu** (§6), so settings are reachable while watching.

Both fix P10. Because the trap is a *bug* of the same family as #452, the fix is
tracked separately (§9) rather than silently widening #506; #506's obligation is
only that the spectate **entry** affordance survives the restyle.

### 5.7 Loading, reconnect, and error states

- **Before the first `LobbyView`** (P8): the Lobby place renders its real
  composition with a **skeleton directory** — three placeholder rows at
  `SCENE_NEUTRALS.lineFaint` with a `micro`-class shimmer (static under reduced
  motion) — plus the header and a working Disconnect. The shape a player is
  waiting for is the shape they see.
- **Pending command** (join/create/submit/ready in flight): the pressed control
  takes the `held` elevation and a busy state; nothing else is disabled, because
  the authoritative answer is a fresh `LobbyView` and the UI must stay
  interactive if it never comes.
- **Rejections**: `lobbyError` keeps its current non-blaming treatment,
  presented in the visual system's illegal/rejected language — a ≤ 3 px
  horizontal shake on the offending control (≤ 200 ms; toast only under reduced
  motion) plus the server's own message (visual-system §7 last row). Never a
  color-only signal, never a blocking modal.
- **Reconnect**: front-door copy per §5.1; the in-lobby arrival needs no cue
  beyond the view itself (the reconnect pulse is an in-match moment, #509).

### 5.8 Transitions

Every row is inside its budget class, none composes past 600 ms, so none needs
an individual skip control; all are interruptible by a newer view.

| Transition | Choreography | Class / duration | Reduced motion |
| --- | --- | --- | --- |
| Front door → Lobby | content column cross-stages; lockup scales into the header; backdrop persists | `staging` 400 ms | cut |
| Lobby → Room | entry column recedes (scale 0.98 + fade), room column rises | `staging` 400 ms | cut |
| Room → Lobby (Leave) | the inverse | `staging` 400 ms | cut |
| Room → Match | pregame column dims and drops; game start assembles | ≤ 400 ms, inside #509's ≤ 800 ms window | cut |
| Match → Lobby (postgame) | scene recedes (scale down + dim) into the lobby surface — visual-system §8 "Return to lobby" | ≤ 400 ms | cut |
| Watching → Lobby | same as postgame return | ≤ 400 ms | cut |
| Row / tile hover, keyboard focus | elevation 0 → 1 | `micro` 120 ms | no tween |
| Selection (deck tile, choice tile, segment) | elevation 2 + blue ring draw-on | `micro` 120 ms | ring appears |
| Ready-bar state change | sentence cross-fades, gold control relabels | `micro` 120 ms | swaps |
| Last-match ribbon in / out | rises + fades | `micro` 120 ms | appears |
| Rejected command | ≤ 3 px shake, 2 cycles | ≤ 200 ms | toast only |

### 5.9 Non-color state, pregame

The §7 rule — no state is color-only at any quality level — restated for these
screens. Every row already has its non-color channel in the shipped lobby; the
restyle must not drop one.

| State | Color | Non-color channel |
| --- | --- | --- |
| Seat identity | seat accent | crest monogram + display name + roster row position |
| Local player | seat accent ring | the "You" tag |
| AI seat | — | the "AI" tag + name |
| Open seat | — | dashed bound + "Open seat" |
| Deck submitted | — | library glyph + "Deck submitted" |
| Ready | — | check glyph + "Ready" |
| Room joinable / full / in progress | — | the button, or the "Full" / "In progress" badge text |
| Occupancy | seat-accent pips | filled vs dashed pip shape + the `n/m` number |
| Next step | gold | the ready bar's sentence + the display-face button treatment |
| Selection | blue | ring + `aria-pressed` |
| Connection state | pill tint | pill word + live pulse shape |
| Match outcome (ribbon) | hue family | the outcome word |

### 5.10 Geometry and input

- **Desktop** 1440×900, floor 1280×800; **tablet landscape** 1180×820 keeps the
  two-column Lobby and Room; below 1180 px wide, and on phone portrait
  (390×844), both collapse to one column with the ready bar still pinned — where
  it matters most.
- **Touch and keyboard are peers with pointer.** Every control ≥ 44 px in every
  input mode; nothing reachable only by hover or drag; tab order follows reading
  order within a place, and a place change moves focus to the new place's first
  heading. The keyboard reachability of the shipped lobby is preserved
  wholesale.
- **125 % text scaling** must not clip the ready bar, the roster rows, the
  segmented pickers, or the room header; these grow in height and wrap rather
  than truncate.
- **Contrast**: text ≥ 4.5:1 against its surface; indicators and state badges
  ≥ 3:1. Seat accents are indicator-class (`#4D7EC9` on `#23262B` is ≈ 3.7:1) —
  they may carry rings, stripes, and pips, and must **never** carry text.

## 6. Where settings live (#505)

The presentation settings surface (`table/PresentationSettings.tsx`, backed by
device-local `table/settings/presentationSettings.ts`) already exists and is
already documented as reachable "from both the front door and the in-match game
menu" (`presentation-budgets.md` §First-run auto-detection,
`clients/web/AGENTS.md`). Today only the first half is true (P7). The direction:

- **Front door** — the existing Settings button, kept.
- **A session menu** in the pregame header, present in **Lobby** and **Room**,
  holding: Display settings, Card art settings (ADR 0024), and Disconnect. This
  is a chrome menu of client-session actions; it never holds a lobby command.
- **The same menu on the game-over overlay and in Watching**, so the span from
  connect to postgame has no settings gap.
- **In-match** stays `GameMenu`, unchanged.

The pregame session menu is #506's to build; adding it to game-over and Watching
rides #452/#509 and the §9 spectate fix respectively.

## 7. Constraints carried

- **No protocol change.** ADR 0012's `LobbyView` / `LobbyCommand` contract is
  untouched: no new command, no new field, no new frame. Every affordance is
  either advertised in `valid_commands` or a client-session action. ADR 0022
  (spectators join and leave through the existing commands) and ADR 0027
  (device-local decks, unchanged) likewise.
- **Zero game logic in the client.** Counts, gate sentences, and pips are
  presentation reads of the view; deck legality stays server-side behind the
  unchanged `submit_deck` gate.
- **Reconstructable from one view.** The only ephemeral additions permitted are
  the last-match ribbon and existing form state, both in the `lobbyError` idiom:
  the UI must rebuild completely from `lobby` alone with them empty.
- **Desktop + touch + keyboard**, 44 px targets, 4.5:1 text / 3:1 indicator
  contrast, 125 % text scaling without clipping.
- **Load budget.** The front-door backdrop is CSS gradients over existing
  tokens: **no new asset bytes**, so the ≤ 4 MB first-match download and ≤ 5 s
  cold-start-to-interactive-lobby budgets are unaffected. Any future front-door
  art is a budgeted decision under `presentation-budgets.md` §Load and asset
  budgets and ADR 0031, not an incidental one.
- **Identity and legality**: procedural geometry only — no card images, no
  official frames, no symbols, no WotC branding anywhere on these screens.
- **File size.** `LobbyScreen.tsx` is already ~1 070 lines. The composition
  above must land split along its seams (stage shell, front door, lobby entry,
  room, roster, ready bar) behind root re-exports, per AGENTS.md.

## 8. Acceptance criteria for #506

Checkable statements #506 can be reviewed against. They extend, and do not
replace, the criteria already listed on #506.

**Visual system end to end**

1. No pregame surface renders the carved-panel treatment: the corner-notch
   panel rules of `screens.module.css` are gone from the front door, lobby, and
   room compositions.
2. Front door, Lobby, and Room render the environment backdrop built from
   `SCENE_THEMES[DEFAULT_SCENE_THEME]`, using the same layer recipe as the match,
   and the backdrop element does **not** re-mount across a place change (test:
   the same node identity persists from front door through room).
3. All colors, shadows, durations, and easings on these screens come from
   `sceneTokens.ts` via a `pregame*` vars builder; no literal hex or duration is
   introduced in the pregame CSS modules.
4. The stage honors the shared quality tier (`on` / `reduced` / `off` ambient
   animation) from `presentationSettings`, and the content layer is identical at
   all three levels.

**Identity**

5. A roster seat wears `SCENE_SEAT_ACCENTS[seat.seat % SCENE_SEAT_ACCENTS.length]`;
   `IDENTITY_ACCENTS` is no longer imported by the lobby.
6. A test pins that the accent a seat wears in the room is the accent that seat
   wears in the match for the same room composition. If the two index mappings
   do not agree, the mismatch is fixed (or filed) rather than papered over.
7. The local player's roster row carries the crest treatment (accent ring +
   monogram, ≥ 44 px) and the "You" tag; identity is never color-only.

**Flow and composition**

8. The room's single gold control is visible without scrolling at 1280×800,
   1180×820, and 390×844, in a full 4-seat room with the deck grid rendered.
9. The ready bar states the gate in words, derived from the current `LobbyView`,
   for each of: no deck submitted; deck submitted and not ready; ready and
   waiting; waiting for players; starting.
10. Exactly one gold affordance is on screen per pregame place, in every state.
11. The empty directory state contains the create affordance rather than
    referring to one elsewhere.
12. The room-id chip and the join field carry matching share instructions.
13. Before the first `LobbyView`, the lobby renders a skeleton directory plus a
    working Disconnect (no dead screen, no bare status sentence).
14. The front door reads *Reclaiming your seat* when `restoreSession()` had a
    stored session for the target address, and *Connecting* otherwise.

**Motion**

15. Every place change runs at the `staging` class (≤ 500 ms) and every
    hover/selection/relabel at `micro`; no pregame sequence composes past
    600 ms.
16. Under reduced motion, every transition, lift, shimmer, and ambient drift is
    zero-duration, with no layout or state difference (assert via the shared
    reduced-motion hook, matching the deck surfaces' existing test).
17. A place change is never gated on animation: the destination's controls are
    hit-testable on the frame the state changes.

**Settings**

18. A session menu holding Display settings, Card art settings, and Disconnect
    is reachable from both the Lobby and the Room, and the front-door Settings
    button is preserved.

**Behavior preserved**

19. Every existing `LobbyScreen.test.tsx` and `ConnectionScreen.test.tsx`
    behavioral assertion still passes, or is migrated with its `data-testid`
    intact: directory browse/join, create with seats/setup, join by id with
    local empty validation, deck tile selection and submit, commander
    designation gated on the advertised flag, ready/unready, leave, copy room
    id, set-name, AI add/remove, deck-builder open/submit/rejection, spectate
    from a directory row, lobby error rendering.
20. No `LobbyCommand`, `LobbyView` field, or protocol type changes in the PR
    (`rune-protocol`, `docs/protocol.md`, and the TS mirror are untouched).

**Accessibility**

21. Every interactive pregame control measures ≥ 44 px in both dimensions at all
    three reference geometries.
22. `sceneTokens.test.ts` is extended with the contrast pairs the pregame
    surfaces introduce (text ≥ 4.5:1, indicators/accents ≥ 3:1); seat accents
    carry no text.
23. At 125 % text scaling, the ready bar, roster rows, room header, and
    segmented pickers wrap without clipping and without shrinking hit targets.

**Postgame target**

24. The Lobby place accepts and renders the last-match ribbon from ephemeral
    store state, and the lobby renders identically and remains fully functional
    with that state absent (the reload case). #506 may ship the ribbon's
    rendering ahead of #452/#509 producing it.

## 9. Follow-ups

Work this direction implies that is **not** #506's:

1. **Spectating is a dead end (P10).** A new fix issue, sibling to #452: a
   Stop-watching control and the session menu in `SpectatorTable`, plus the
   store transition that clears `spectatorView` on `leave`. No protocol change
   (`lobby/commands.rs:512-524` already handles it). Should land before or with
   the #506 restyle so the spectate entry point is not a trap.
2. ~~**The last-match ribbon's producer.**~~ **Closed by #506.** #452's
   `GameStore.leaveGame` now writes the ephemeral outcome/opponents/setup record
   on the same transition that clears `view`, reading it from the terminal
   `GameView` plus the room's last `LobbyView` *before* the teardown destroys
   both (`store.ts` `lastMatchOf`). It is suppressed when there is no result to
   report — leaving a spectated game, or an exit with no server to return to.
   #509 stages the moment; it does not need to produce the record.
3. **A real rematch.** Returning a finished room to `gathering`, or a
   `rematch` command that mints a new room seeded with the same seats, is a
   protocol change (ADR 0012 amendment + `rune-protocol` + `docs/protocol.md` +
   TS mirror in one PR). Until then "Play again" is create-and-share.
4. **Sharing beyond a room id.** A join link or a short human-readable room code
   would fix P6 properly rather than explaining it. Both are protocol-adjacent
   (the code is a server-minted id) and are deferred.
5. **Postgame history or stats.** Explicitly out of scope here — nothing in the
   protocol survives a room's reclamation, so any history is a new persistence
   decision (ADR 0027's deferred server-side half is the nearest precedent).
6. **The lobby staged on the plane** (alternative C) as a later flourish, if the
   front door ever earns dedicated art under the asset budget.
7. **`ui-design-notes.md` §Front door** gets its supersession note when #506
   lands the composition, following ADR 0029's pattern of keeping the shipped
   record and marking the direction superseded.
