# Web client agent guide

The browser client. It renders a server-sent view and returns an identifier the server issued;
it decides nothing about the game. Read [`docs/brief.md`](../../docs/brief.md) and
[`docs/protocol.md`](../../docs/protocol.md) before changing anything here.

## Hard rules

- **Zero game logic.** The client renders `GameView` and sends back an `action_id` from
  `valid_actions[]`. It never computes legality, cost, effect, or which targets are legal. If
  you find yourself asking "can this be played?", the answer belongs to the server — the view
  either says so or the question is not the client's to ask.
- **The entire UI must be reconstructable from one `GameView` plus a pending prompt.** No
  client state is load-bearing across messages. A refresh mid-game must produce the same screen
  as the message that preceded it. Anything you cache is a rendering optimization that must be
  safe to throw away.
- **`src/protocol.ts` mirrors `crates/sage-protocol`, which is the authority.** A wire change
  updates the Rust types, `docs/protocol.md`, and this mirror in the same PR. Schemas declare
  no defaults: absence is a fact about what the server said, and the parity test depends on it.
- **Tolerate unknown fields; declare every known one.** A newer server may send fields this
  client does not know. Parsing strips them rather than failing — which is why the parity test
  asserts a parsed fixture equals the fixture, so a field the mirror is *missing* fails loudly
  instead of vanishing.
- **One browser, named by the pin.** `make e2e-browser` provisions the revision the pinned
  `@playwright/test` asks for — idempotent, and a no-op where it already exists (the CI
  container). Never fetch an *unpinned* browser (`playwright@latest`, a caret range, a
  hand-picked revision) and never pin an `executablePath`: either lets a local run and CI
  drive different binaries, which is the failure ADR 0011 is about.
- **[`docs/client-design.md`](../../docs/client-design.md) is the layout authority.** Read it
  before changing anything that occupies space. Its one-line summary: *zoom, resolution, and
  aspect are the same problem; no region of the board scrolls vertically or ever grows a
  scrollbar, and a full row pans sideways instead; text is fitted, never truncated.* A region's
  size is a function of the **viewport alone**, never of what is in it; a card's size is the
  height of the region it is in; and a count is absorbed by the row.
- **That document describes `clients/prototype`, and this client is built to it.** The prototype
  is the authority above the document: where the two disagree, the prototype is what the client
  follows, and the document's §8 lists every rule the prototype retired. The prototype is still
  where a *new* screen is tried first — nothing ships from it, and it is not this client's
  scaffolding. That ordering settles **appearance only**: it never outranks a hard rule on this
  list or an accepted ADR. The full statement is in
  [`docs/brief.md`](../../docs/brief.md#which-document-wins).
- **Dark, declared rather than followed.** A card is an object lying on a surface, it needs a
  ground darker than itself, and maintaining a light table as well is how neither gets good.
- **An overlay renders the join and nothing else.** It is `aria-hidden`, because a drawn line is
  not readable, and the text under it stays the copy every player can reach. No fact may be
  available only as a drawn line. WebGL stays out until something demonstrates it is needed.

## Layout

- `src/protocol.ts` — the wire mirror. Schemas plus the types inferred from them, one
  declaration each. `src/protocol.test.ts` checks it against
  `crates/sage-protocol/fixtures/`, the same files the Rust tests pin.
- `src/frame.ts` — classifies an untagged server frame. Server frames carry no envelope, so
  the discriminators are structural and order-sensitive; the rules are the protocol's.
- `src/normalize.ts` — turns wire absence into values a renderer can use. Every documented
  default lives here, so no component invents its own reading of a missing field.
- **The arrangement is CSS.** There is no scene, no packer and no measured fitting module: the
  board is a grid of `fr` rows (`styles/board.css`, with `panel.css` and `dock.css` beside it
  as one stylesheet cut for size — the three must stay imported back to back and in that order,
  because in CSS order is behavior), a card is sized from the region it is in, and a row that
  runs out of width pans. `scene.ts`, `pack.ts`, `fit.ts` and `overlay.ts` were
  the arithmetic that did this before and are gone; what survived them is `board.ts`, which
  answers which rows a field draws and what share of its height each takes.
- `src/board.ts` — a battlefield, as rows. Groups permanents into creatures, other permanents,
  and lands from `CardView.card_types`, which the **server** states beside the type line it
  renders (`docs/protocol.md`) precisely so nothing here parses `"Artifact Creature — Thopter"`.
  What this module decides is the other question — where to draw a permanent that is more than
  one thing — and that is presentation with no rules content: a creature-land is drawn with the
  creatures, because a creature is what a player scans for. Rows mirror across the table so both
  sets of creatures meet at the dividing line, and inside a row the server's order is kept.
  `fieldRows` is the other half and the one the board draws from: **creatures and lands are drawn
  whether or not anything is in them**, because a row that appeared with the first creature would
  resize every land under it (§5). The third row appears only when the server states a kind of
  permanent that is neither, which is a fact about the game rather than a count.
- `src/dock.ts` — what the controls are currently *for*, in one word: your move, the game is
  asking, in flight, confirm this, waiting, over. Decided in that order — a finished game outranks
  everything, a destructive question outranks a submission in flight, and a submission in flight
  outranks an action list, because the player has already answered. The dock draws it as colour
  *and* says it in words: a colour nobody has learnt, a colour two of which look alike, and a
  colour a screen reader cannot see all say nothing on their own. `barTone` is what the bar is
  actually tinted by, and it is a different question with a different answer: **where in the turn
  you are** (§6.5) — green on the bookends, blue in a main phase, red once combat is live — so the
  bar changes colour when the situation changes rather than flickering between two shades of
  "asking" inside one step.
- `src/mana.ts` — a printed cost as symbols, and the tint a frame is washed in. Tokenizing
  `{2}{G/U}` is presentation of a string the server sent; the tint is **the colours of the pips
  that were printed** and deliberately none of the rules concepts it resembles — not colour, not
  colour identity, not devotion. A card whose colour comes from anywhere else tints neutral,
  which is the honest answer for a client that was handed a cost string, and is why the tint is
  only ever a wash under text that states the real thing. `inlineSymbols` does the same for a
  *sentence*: the server writes `{T}: Add {G}.` and a player reads pips, so rules text draws them
  too — the prose is still the server's, byte for byte, and only the parts between braces change.
- `src/keys.ts` — a keypress as an intent, and nothing else. Whether the intent is currently
  possible is answered where the view is, out of `valid_actions`. The skip keys are **not** a
  client-side auto-pass: each sends one `set_stops` and one pass, and the pacing that follows is
  the server's settle acting on a preference it stores (ADR 0010). Nothing here loops or waits.
- `src/art/` — ADR 0012's pipeline, keyed by `functional_id`. The art window with nothing in it
  is a field tinted by the printed cost rather than a generated composition — the procedural
  layer is retired (§8). `settings.ts` is the device preference, off by default, and its `style`
  is the three faces of §9.6: `frame` fetches nothing at all; `source.ts` is the pluggable lookup
  (Scryfall today) and the only thing that ever crosses it is a card name; `store.ts` is the
  registry — one request per card ever, one at a time, spaced, with resolved URLs cached
  device-local. **Art is cache, never state**: every test here is offline, and the whole UI must
  still reconstruct from one `GameView` with the store empty.
- `src/card-face.ts` — reduces the five card-shaped projections (`CardView`, `Permanent`,
  `StackItem`, `Emblem`, `CatalogCard`) to one `CardFace`. Every surface renders that and nothing
  else, so the hand, the board, the stack, and the deck builder cannot disagree about the same
  object. Add a card-presentation rule here, not in a component.
- `src/table.ts` — joins the seats. Life and library arrive as `me` for you and `opponents[]`
  for everyone else, the piles as three arrays keyed by player, commander state as three more;
  they become one `Seat[]` here so no panel rebuilds that join or gets its absences wrong. A
  mana pip's `*` suffix is read here too: it is a wire encoding for **restricted** mana (CR
  106.6), not part of the symbol, and the wire says only *that* a pip is restricted — never what
  to — so a surface may mark it and must not guess at the condition.
- `src/relations.ts` — what points at what: attacks, blocks, attachments, targets, and the
  source of an ability, joined from the identifiers the server stated and indexed **both ways**.
  One direction is not enough to draw a board — a blocker knows what it blocks and the attacker
  knows nothing about being blocked — so the reverse is derived here once rather than by each
  surface scanning the battlefield. Every edge is a stated id; nothing is concluded from rules
  text or the log. It joins the **names** too, because an id is not a name: `entityNames` is
  every `(id, name)` pair the view states, and a trail draws a word from that or draws nothing.
  **No server identifier reaches a surface a player reads** (`docs/client-design.md` §9.2), and
  an end the view named nowhere is never filled in with a kind concluded from the relationship.
  `relationNote` is the same lines as one sentence, and it is what makes the drawn arrow safe: an
  overlay is `aria-hidden`, so every line it draws is also in the accessible name of the object it
  belongs to.
- `src/arrows.ts` — which stated relationships are drawn over the board, and in which of the two
  tones. The same split `overlay.ts` made and the reason it is kept: what decides *what* is
  related knows nothing about pixels. An attachment is not drawn — the card behind the card says
  it — and neither is an ability's source, which the stack item already carries.
- `src/menu.ts` — whether an object's own actions belong *at* the object. The dock's list, from
  the same `actionsFor`, opened by the click that already produced `{kind: 'select'}` — no new
  gesture, and deliberately **not** a right-click menu: right-click is spent on the inspector,
  which is what keeps reading free, and it is the one gesture here with no keyboard equivalent.
  An armed draft, a confirmation, a submission in flight, and an empty selection each outrank it,
  because in all four something else already owns the next click.
- `src/motion.ts` — what changed between the last two views: ids the board draws that it did not,
  which card moved zones, and how far a seat's life moved. Pure, so an animation is a *transition
  between* two reconstructable states and never a third of its own; a reconnect resets it, because
  a board that moved while the socket was down was not watched. **Following a card is the server's
  join** — `physical_card`, the physical card (CR 108.1) a permanent or a spell is a projection of
  — and matching by `name` or `functional_id` is a bug, not a fallback: two Forests agree on both,
  so a name join is the client deciding which one moved. Following a card concludes **nothing**
  about an object: CR 400.7 makes the two ends two different objects, and no counter, damage,
  attachment, or control crosses the gap.
- `src/turn.ts` — turn flow: the steps of a turn, who the game is waiting for, where it will
  stop for this seat next time, and what a settle already did on their behalf. The stop controls
  read the *effective* lists the server reflects and send back the whole preference, because
  `set_stops` replaces it and is never a delta — nothing about a stop is stored client-side. A
  settle's path may cross a turn boundary, so only entries whose `turn` matches the view's may
  mark a step in it. `passedEvents` is the other half: the path says *where* the settle acted
  and never *what happened there*, which is the half a player reads — a spell cast, resolved,
  and killing a creature inside one settle is three log events and no step anybody recognises.
  The events are already in the log; which of them this receiver missed is the server's to say
  (`auto_passed_from`), so this filters and never infers.
- `src/game-log.ts` — the wording for one log event, and the class of thing it is so a column of
  sentences can be scanned. Events carry data, never prose.
- `src/lobby.ts` — the pre-game joins: the directory as rows, and the seats of the table you are
  at. A row's button exists because `valid_commands` currently offers `join_room` or
  `spectate_room`; occupancy only chooses **which** advertised command it leads with. A seat's
  status is the stated flags restated, and what a table is waiting on is which of them is still
  false — the gate that starts a game is the server's.
- `src/deck.ts` — a deck under construction, and the catalog it is built from. The wire wants a
  flat list with duplicates repeated; a person wants counted entries, so the draft holds counts
  and expands at submission. A draft also holds the cards *beside* the deck and `expand` leaves them
  out — **the wire has no sideboard**, so one is this device's own note (ADR 0018) — and a commander
  is a designation over the deck list rather than a list of its own, so its last copy leaving clears
  it. **It computes no legality**: the rules strip quotes the numbers the
  format published, the size note is arithmetic on a count, and nothing reads a `type_line` to
  decide what a card *is* — which is why the copy limit is displayed and never enforced and the
  commander picker offers the deck's own cards. The verdict is the server's `LobbyRejection`.
- `src/builder.ts` — the deck editor's reading of its draft: the pool narrowed to the search, the
  deck cut into columns, and the curve/colour/type summaries. Every number counts what the server
  described, and none is a verdict. A land is off the curve and in its own column rather than at
  zero: counting it as zero puts a deck's land base in the column read as "free spells".
- `src/dck.ts`, `src/deck-store.ts` — decks as files, and decks on this device (ADR 0018). Parsing
  is pure and **reports every name the catalog does not hold** instead of dropping it. The store is
  device storage in the manner of `connect.ts`: injectable, and absent or unreadable is a working
  answer — storage off means decks are lost on reload, never a screen that will not open.
- `src/submission.ts` — composes one `choose_action`. Bookkeeping over slots the server
  advertised, never rules reasoning.
- `src/interaction.ts` — what one click *means*: which objects own an action, which slot a click
  answers, what is highlighted, and which submission is still unanswered. One gesture reaches
  every object and resolves in a fixed order (fill a slot → inspect the selected → **take the
  one action the server offered** → select, where it offered more → inspect), so the hand, the
  board, and a pile cannot behave differently. The count of actions the server attached to an
  object is the whole of the take-versus-select rule: a "primary" action ranked by type would be
  this client interpreting what an action *does*, which is exactly the reasoning that does not
  live here. Reading is not a click at all — the pointer previews, the right-click inspects —
  which is what makes a one-click action safe. A click is routed to the slot the server listed
  that id in, never to a cursor the client advances; that is what lets one action ask "who
  attacks" and "what does each attack" at the same time.
- `src/connect.ts` — where this client connects and who it says it is, both device-local in the
  manner of ADR 0012's art preference. The server list is **client-side configuration**: the
  protocol has no server directory and this is not one, `PUBLIC_SERVERS` is empty because none is
  public yet, and the custom entry is what keeps a table in a file from being a limitation. It
  **extends** `socket.ts`'s address precedence rather than replacing it — `?server=` still
  outranks everything, then this device's choice, then the build-time value and the page's origin.
- `src/socket.ts`, `src/useSession.ts` — the connection, and the latest frame it delivered. A
  dropped socket retries on its own and says `hello` with the stored token, which is what
  reclaims a held-open seat; the server answers by putting the connection back on whatever
  contract that seat is on, so a reconnect mid-game may deliver no `LobbyView` at all. **The
  token is claimed once and then defended** — a fresh socket is issued its own identity before
  its `hello` is read, so a lobby frame carrying a different session is routine during a
  reconnect and adopting it would discard the token that owns the seat. Leaving a finished game
  is the deliberate opposite: forget the token, and connect as somebody new.
- `src/ui/card/` — the card, which is one drawing everywhere it appears (§6). `Card.tsx` is one
  SVG in the printed grid — a title bar the name leads and the cost follows, an art window, a
  type bar, a text field, the stat on a plaque — and **there is no variant to pass**: the
  surface's stylesheet states the box, and every run of text is fitted inside the card's own grid
  by bisection, so a hand card and a board card of different sizes are the same drawing. `Pips.tsx`
  draws every mana symbol from scratch in a 100×100 box (never an official symbol, never a
  downloaded one) and hands assistive technology words instead; `Symbols.tsx` does the same for a
  sentence the server wrote. `peek.ts` is press-to-read, `scrollStrip.ts` is the pan a full row
  and a full hand share, and `art.tsx` is the provider the whole app is wrapped in, because the
  preference and the cache belong to the *device*.
- `src/ui/game/` — the table. `Board.tsx` composes and derives; a surface receives answers, never
  the view, so none of them can grow a second reading of it. The arrangement is the stylesheet's
  (`styles/board.css`): the opponents band takes one share per row of seats it holds, your half
  takes the rest, the action bar is permanent above the hand, and the side column is fixed beside
  all of it. Chrome is at the edges, the board is in the middle, the turn is a strip that is
  always drawn and is also where stops are set, and one gear opens everything about the *device*.
  The bar carries only what the board cannot answer — a tally, a commit, a cancel, and a control
  for a subject no surface drew — which is what keeps a question with twenty blockers the same
  height as one with two. A relationship the server projected is drawn twice and the copies are
  not alternatives: an arrow on `Arrows.tsx`'s overlay, which takes no clicks and is hidden from
  assistive technology, and the same words in the accessible name of the object that carries it.
  Every surface tags what it draws with `data-anchor`, which is how a line finds its two ends,
  where `ObjectMenu` opens, and what `Motion` moves — so a surface gets all three by tagging.
  Public piles open as a dialog over the table and may scroll, because a pile is not the board.
- `src/ui/pregame/` — the screens in front of a game (§9), and `Pregame.tsx` holds what they share:
  the catalog, the deck this device has chosen, and the one dialog that loads a deck wherever it is
  opened from. **Which screen is on is the server's answer** — a `LobbyView` with a `room` is a
  table you are at, one without it is the directory — with the deck editor the one exception: a
  device-local screen over whichever of the two you were on, which puts you back there on the way
  out. The topbar of each screen is its navigation, and `ui/Settings.tsx` is a dialog over whatever
  you were already on. `DeckEditor.tsx` is the *small* edit at a seat — the sideboard line, with a
  way through to the full editor — and a deck changed there is submitted, because the table is
  holding a seat for whatever it is now.
- `src/ui/deck/` — the deck editor as its own screen (§9.7): pool and search above, options bar
  between, `DeckArea.tsx`'s columns below, sidebar carrying the board's card viewer, the file
  controls and `DeckStats.tsx`. **Everything the options bar changes is a reading**, never the
  draft. `TitleBand.tsx` is why `Titles` is honest — the printed title bar is one drawing with three
  callers, so a decklist and a card cannot disagree. Take a palette class from `tint.ts`, never by
  composing `card-${tint}`: two names do not match their tint, and a hand-built one resolves to no
  palette and draws a black box.
  `ui/Connect.tsx` asks who you are and where, with the gear, so card art can be chosen before
  ever joining. Nothing here is a native form control: a `<select>` clips its own arrow at 120%
  zoom, so every choice is a segmented control, a radio list, or a picker dialog. **Two panels
  have nothing behind them** — chat, and who is in the lobby or watching a table — and they are
  drawn saying so rather than left out; nothing in them is faked.
- `src/index.css`, `src/styles/` — the dressing, and **the arrangement**: every rule in it comes
  from `clients/prototype`. `material.css` is §5.5 — the panes, the recesses and the buttons every
  screen is built out of — and comes first because everything else is drawn on top of it; then the
  card, the board, one file per pre-game screen, `overlay.css` for the three surfaces that layer
  over any of them, and `narrow.css` last, because a phone-sized and a short-window board are the
  same rules with less room and they have to win. Imports come first, as the cascade requires.
  Class names are shared across screens on purpose — a `.card` is the same card everywhere, an
  `.action-bar` does the same job before a game as during one — so a rule added here is a rule
  added to every screen that borrows the name.
- `e2e/smoke.spec.ts` — the blocking gate: one path against the real server.
- `e2e/*views.spec.ts` — the non-blocking tier: committed fixtures replayed over an intercepted
  socket, no server involved. Four files sharing `e2e/frames.ts` — the board, the pre-game
  screens, the card, and the sweep across viewports; the `views` project matches on the suffix.

Keep logic out of components. Anything worth a test belongs in one of the modules above, which
are pure and need neither React nor a browser.

## Commands

Run from the repository root:

- `make client-check` — everything the `Client` CI job runs: format, lint, types, tests, build.

- `make e2e-smoke` — the blocking browser gate, against a real `sage-server`.
- `make e2e-views` — the broad, non-blocking browser tier. Needs no server and no Rust.

Or from this directory: `npm run lint`, `npm run typecheck`, `npm test`, `npm run build`,
`npm run dev`, `npm run e2e`.

When a browser test fails, `npx playwright show-trace test-results/<dir>/trace.zip` replays it.
CI uploads the same traces as artifacts on failure.

`make check` is engine-only and does not run any of this — an engine change must not need node
installed. `make verify` runs both.
