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
- **Never run `playwright install`.** Consume the preinstalled browser via
  `PLAYWRIGHT_BROWSERS_PATH` with `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`, pinning
  `executablePath` when the pinned package disagrees with the image (ADR 0011).
- One layout: desktop landscape, mouse and keyboard, two players. Plain DOM and CSS, no WebGL.
  Responsive breakpoints, touch input, and more than two seats are not in scope — adding them
  early is how the last three layouts happened.

## Layout

- `src/protocol.ts` — the wire mirror. Schemas plus the types inferred from them, one
  declaration each. `src/protocol.test.ts` checks it against
  `crates/sage-protocol/fixtures/`, the same files the Rust tests pin.
- `src/frame.ts` — classifies an untagged server frame. Server frames carry no envelope, so
  the discriminators are structural and order-sensitive; the rules are the protocol's.
- `src/normalize.ts` — turns wire absence into values a renderer can use. Every documented
  default lives here, so no component invents its own reading of a missing field.
- `src/submission.ts` — composes one `choose_action`. Bookkeeping over slots the server
  advertised, never rules reasoning.
- `src/socket.ts`, `src/useSession.ts` — the connection, and the latest frame it delivered.
- `src/ui/` — the screens. Grey-box on purpose: structure and legibility, no visual design.

Keep logic out of components. Anything worth a test belongs in one of the modules above, which
are pure and need neither React nor a browser.

## Commands

Run from the repository root:

- `make client-check` — everything the `Client` CI job runs: format, lint, types, tests, build.

Or from this directory: `npm run lint`, `npm run typecheck`, `npm test`, `npm run build`,
`npm run dev`.

`make check` is engine-only and does not run any of this — an engine change must not need node
installed. `make verify` runs both.
