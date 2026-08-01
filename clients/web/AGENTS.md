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
- One layout: desktop landscape, mouse and keyboard, two players, and **dark**. The scheme is
  declared rather than followed: a card is an object lying on a surface, it needs a ground darker
  than itself, and maintaining a light table as well is how neither gets good. Plain DOM and CSS,
  no WebGL. Responsive breakpoints, touch input, and more than two seats are not in scope —
  adding them early is how the last three layouts happened.

## Layout

- `src/protocol.ts` — the wire mirror. Schemas plus the types inferred from them, one
  declaration each. `src/protocol.test.ts` checks it against
  `crates/sage-protocol/fixtures/`, the same files the Rust tests pin.
- `src/frame.ts` — classifies an untagged server frame. Server frames carry no envelope, so
  the discriminators are structural and order-sensitive; the rules are the protocol's.
- `src/normalize.ts` — turns wire absence into values a renderer can use. Every documented
  default lives here, so no component invents its own reading of a missing field.
- `src/board.ts` — a battlefield, as rows. Groups permanents into creatures, other permanents,
  and lands from `CardView.card_types`, which the **server** states beside the type line it
  renders (`docs/protocol.md`) precisely so nothing here parses `"Artifact Creature — Thopter"`.
  What this module decides is the other question — where to draw a permanent that is more than
  one thing — and that is presentation with no rules content: a creature-land is drawn with the
  creatures, because a creature is what a player scans for. Rows mirror across the table so both
  sets of creatures meet at the dividing line, and inside a row the server's order is kept.
- `src/mana.ts` — a printed cost as symbols, and the tint a frame is washed in. Tokenizing
  `{2}{G/U}` is presentation of a string the server sent; the tint is **the colours of the pips
  that were printed** and deliberately none of the rules concepts it resembles — not colour, not
  colour identity, not devotion. A card whose colour comes from anywhere else tints neutral,
  which is the honest answer for a client that was handed a cost string, and is why the tint is
  only ever a wash under text that states the real thing.
- `src/keys.ts` — a keypress as an intent, and nothing else. Whether the intent is currently
  possible is answered where the view is, out of `valid_actions`. The skip keys are **not** a
  client-side auto-pass: each sends one `set_stops` and one pass, and the pacing that follows is
  the server's settle acting on a preference it stores (ADR 0010). Nothing here loops or waits.
- `src/art/` — ADR 0012's pipeline, keyed by `functional_id`. `procedural.ts` seeds a
  composition from a card's own identity so the art window is never empty and nothing downloads;
  `settings.ts` is the device preference, off by default; `source.ts` is the pluggable lookup
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
  text or the log.
- `src/turn.ts` — turn flow: the steps of a turn, who the game is waiting for, where it will
  stop for this seat next time, and what a settle already did on their behalf. The stop controls
  read the *effective* lists the server reflects and send back the whole preference, because
  `set_stops` replaces it and is never a delta — nothing about a stop is stored client-side. A
  settle's path may cross a turn boundary, so only entries whose `turn` matches the view's may
  mark a step in it.
- `src/game-log.ts` — the wording for one log event, and the class of thing it is so a column of
  sentences can be scanned. Events carry data, never prose.
- `src/lobby.ts` — the pre-game joins: the directory as rows, and the seats of the table you are
  at. A row's button exists because `valid_commands` currently offers `join_room` or
  `spectate_room`; occupancy only chooses **which** advertised command it leads with. A seat's
  status is the stated flags restated, and what a table is waiting on is which of them is still
  false — the gate that starts a game is the server's.
- `src/deck.ts` — a deck under construction, and the catalog it is built from. The wire wants a
  flat list with duplicates repeated; a person wants counted entries, so the draft holds counts
  and expands at submission. **It computes no legality**: the rules strip quotes the numbers the
  format published, the size note is arithmetic on a count, and nothing reads a `type_line` to
  decide what a card *is* — which is why the copy limit is displayed and never enforced and the
  commander picker offers the deck's own cards. The verdict is the server's `LobbyRejection`.
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
- `src/socket.ts`, `src/useSession.ts` — the connection, and the latest frame it delivered. A
  dropped socket retries on its own and says `hello` with the stored token, which is what
  reclaims a held-open seat; the server answers by putting the connection back on whatever
  contract that seat is on, so a reconnect mid-game may deliver no `LobbyView` at all. **The
  token is claimed once and then defended** — a fresh socket is issued its own identity before
  its `hello` is read, so a lobby frame carrying a different session is routine during a
  reconnect and adopting it would discard the token that owns the seat. Leaving a finished game
  is the deliberate opposite: forget the token, and connect as somebody new.
- `src/ui/` — the screens. The table is drawn: `Card.tsx` is one frame at five budgets, at a
  printed card's proportions, with a name band, an art window, a text box, and the stat in the
  corner; `Mana.tsx` draws a cost as the project's own discs (never an official symbol, never a
  downloaded one) and hands assistive technology words instead; `CardArt.tsx` fills the window
  with the procedural composition and lays a player-supplied illustration over it; `art.tsx` is
  the provider the whole app is wrapped in, because the preference and the cache belong to the
  *device* and a lobby, a builder, and a table all draw the same card. The lobby is still grey
  box.
  `src/ui/game/` holds the table surfaces — header, seat panels, the two battlefields, the
  stack rail, hand, action dock, side panel. `Game.tsx` composes them and derives what they
  need; a surface receives answers, never the view, so none of them can grow a second reading
  of it. The composition is fixed, full-viewport, and two-player: chrome at the
  edges and the board in the middle. The turn is a rail down the left, because twelve steps is a
  lot of band to spend above a board and none of it is spent there; the top is one line about the
  match; the bottom is your hand and the button that moves the game; and one gear opens
  everything that is about the *device* rather than the board — pace, keys, card art — because a
  header carrying each of them separately is a header nobody reads. Every region is bounded so a
  full board scrolls inside its own area and never pushes the action dock off the screen. The
  dock follows the click — the selected object's actions, or the questions an armed action is
  asking — and keeps two lists beside that: the global actions no object owns, and a disclosure
  of every action, so a subject no surface happens to draw can still be reached. A relationship
  the server projected hangs under the object that carries it as a trail of controls, and
  looking at an object emphasises what it relates to — tracing follows the *look*, not the
  click, because the objects most worth tracing own no action and a click on one opens the
  inspector over the board the relationship crosses. Public piles open beside the table, never
  over it; a hidden zone is a count with nothing to open. The header carries the whole turn as a
  row of steps, and that row is also where stops are set — a preference divorced from the strip
  it applies to is one nobody edits. The end of a match is the one panel that layers over the
  board, and the one action asked twice before it is sent.
  Three surfaces exist because reading a card had to stop costing a click: `CardPreview` over
  the side column follows the *look*, `Shortcuts` writes the keys down, and `PacePresets` puts
  the whole stop preference on the same row as the strip it applies to — each shortcut printed
  on the control that duplicates it, because a shortcut nobody can find is a shortcut nobody
  uses.
  `src/ui/lobby/` holds the pre-game surfaces — table directory, table form, seat roster, deck
  panel, deck builder — with `Lobby.tsx` composing them the same way. Which of the two pre-game
  screens is on is the server's answer, not a client-held phase: a `LobbyView` with a `room` is a
  table you are at, one without it is the directory. One form serves creating *and* editing a
  table, because `create_room` and `update_room` both carry a whole `RoomConfig`; every choice in
  it — the formats, that format's seat range, the AI kinds — comes from the `CatalogView`, which
  is requested per socket and is the reason nothing here hardcodes a `game_setup` id. The builder
  draws catalog cards through the same `Card` the table does. Submit is offered whenever
  `submit_deck` is advertised and is never gated on the client's own arithmetic.
- `src/index.css`, `src/styles/` — the grey box, split along the surfaces it dresses: the page
  itself, then `cards.css`, `game.css`, `lobby.css`. Imports come first, as the cascade requires.
- `e2e/smoke.spec.ts` — the blocking gate: one path against the real server.
- `e2e/*views.spec.ts` — the non-blocking tier: committed fixtures replayed over an
  intercepted socket, no server involved. More than one file, sharing `e2e/frames.ts`; the
  `views` project matches on the suffix.

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
