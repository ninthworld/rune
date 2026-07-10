# MTG Engine — Project Brief

## Project Overview

An open-source Magic: The Gathering implementation consisting of a high-performance rules engine server and a platform-agnostic client. The project is divided into two largely independent components: a Rust server that owns all game logic, and a React web client that owns all rendering and user interaction. These communicate over a minimal JSON/WebSocket API.

The design philosophy is that the client is intentionally "dumb" — it only renders what the server tells it, and only allows actions the server has confirmed are legal. Zero game logic lives in the client.

---

## Component 1: Rust Server

### Purpose
The server is a match-hosting platform. It manages player sessions, lobbies, rooms, and active games. The rules engine is a component inside the server, not the server itself.

### Architecture: Three Layers

**Layer 1 — Matchmaking & Lobby**
- Owns: WebSocket connections, player identity/auth, room registry, chat
- Does NOT know: card rules, game state, phases, stack
- Implemented as shared state: `Arc<RwLock<ServerState>>`

**Layer 2 — Room / Session**
- One Tokio async task per active room
- Owns: player slots, ready state, reconnection logic, spectators, turn timers
- Does NOT know: what a land is, what the stack does, what a trigger is
- Runs an event loop: `tokio::select!` over player actions, turn timer, disconnect events
- Owns exactly one instance of the rules engine

**Layer 3 — Rules Engine**
- Owns: all game state, phase FSM, stack, triggers, zones, the layer system
- Does NOT know: network, players, timers, rooms, reconnection
- Pure functions only: `apply_action(state, action) -> GameState`
- Single-threaded per game (MTG is sequential — no parallelism needed inside a game)

### Concurrency Model
- Runtime: Tokio async
- One lightweight async task per connected client (~2KB each)
- One async task per active room (~1MB each including game state)
- Tokio work-stealing scheduler multiplexes all tasks across OS thread pool
- You never manage threads manually
- A single mid-range server (16GB RAM, 8 cores) can host ~10,000+ concurrent games

### Scalability
- Single-node is sufficient up to tens of thousands of concurrent games
- The room registry is hidden behind a trait so it can be swapped from in-process to Redis-backed without changing anything else
- Horizontal scaling only needed well past 10,000 concurrent games

### Deployment Modes
The same Rust codebase compiles to three targets:

1. **Cloud server** — binary running on a VPS/cloud instance, clients connect via WebSocket
2. **Bundled desktop** — Rust binary bundled with the desktop app, auto-launched on startup, Flutter/React UI connects via localhost socket
3. **Mobile in-process** — Rust compiled as a native library (.so on Android, .a on iOS), called via FFI inside the app process using `flutter_rust_bridge` (if using Flutter) or equivalent
4. **Web offline** — Rust compiled to WebAssembly, runs in a Web Worker thread inside the browser, same Rust code with different bindings

### The API (Minimal by Design)

The entire client-server protocol is two message types:

**Server → Client (per player, personalized):**
```json
{
  "my_hand": [...],
  "opponents": [{ "player_id": "...", "hand_size": 4, "life": 20 }],
  "battlefield": [...],
  "stack": [...],
  "graveyards": [...],
  "phase": "combat_damage",
  "priority_player": "player_1",
  "valid_actions": [
    { "id": "a1", "type": "pass_priority", "label": "Pass" },
    { "id": "a2", "type": "activate_ability", "source": "card_xyz", "label": "Tap for mana" }
  ],
  "action_deadline": 30
}
```

**Client → Server:**
```json
{ "type": "choose_action", "action_id": "a2" }
```

That is the entire API. The client never sends game logic — it only sends which pre-validated action the player chose.

Any client — React UI, CLI, LLM agent — speaks this same two-message protocol. An LLM receives the GameView as a JSON prompt and returns an action_id. A CLI prints the actions as a numbered list and reads stdin.

### Rules Engine Design: Immutable State Machine

**Core principle:** The game state is an immutable value type. Every action produces a new state; the old one is never modified.

```rust
#[derive(Clone)]
struct GameState {
    turn: u32,
    phase: Phase,
    priority: PlayerId,
    players: Vec<PlayerState>,
    battlefield: HashMap<PermanentId, Permanent>,
    stack: Vec<StackObject>,
    // ... all game state
}

fn apply_action(state: &GameState, action: Action) -> GameState {
    let mut next = state.clone();
    // apply changes to next
    next
}
```

**Why immutable:**
- Undo is trivial — history is just `Vec<GameState>`, go back to any index
- AI tree search: clone a state, simulate 1000 futures, discard, no cleanup
- Replay: store `Vec<Action>`, replay through `apply_action` to reconstruct any moment
- Network resync: send the client the current `GameState`, they're immediately correct
- No listener/observer bugs — nothing is registered, everything is computed fresh

**No listeners or observers.** Instead of cards reacting to events (push-based), the engine computes everything from current state on demand (pull-based):

- **Triggered abilities:** After every `apply_action`, a pure function diffs `before` and `after` states and collects all triggers that should now exist
- **Replacement effects:** Events pass through a pure pipeline of `Event -> Event` transformations before touching state
- **Continuous effects / Layer system:** Permanents store no computed characteristics. `characteristic(state, permanent_id)` is a pure function that runs the layer system fresh on every call. Layers run in order: copy effects → control → text → type → color → abilities → power/toughness

**Permanent identity:** Each time a card enters the battlefield it receives a fresh `PermanentId`. Zone tracking uses these stable IDs, not object references. There are no ZoneChangeCounters because the ID changing on zone entry is the mechanism — the "second time on battlefield" has a different ID from the "first time."

**Mutate / complex mechanics:** A mutated permanent is a `mutation_stack: Vec<CardId>` with a `top_index`. The top card defines name/mana cost/type/PT. All cards contribute abilities. `characteristic()` reads the whole stack. The PermanentId never changes through mutation. No UUID patching, no stale listeners.

**Action pipeline (every action follows this):**
1. Validate: is this action in `valid_actions(state)`?
2. Clone state → next_state
3. Apply raw action to next_state
4. Run `apply_replacements()` on any events generated
5. Run `state_based_actions()` loop until stable (creatures die, etc.)
6. Run `collect_triggers(before, after)`, push new triggers onto stack
7. Return next_state

### AI Integration

Two-tier AI:

**Rule-based AI** — fast, deterministic, in-process. Handles obvious plays: winning moves, mana efficiency, basic combat math. Runs synchronously, no latency.

**LLM agent** — async, called when rule-based AI defers. Receives the GameView JSON as a prompt, returns an action_id. Timeout fallback to rule-based if LLM is slow. The LLM plays via the same API any human client uses — it's just another client that reads JSON and responds with an action_id.

### Legal Considerations

- Implementing the rules is legal (game mechanics are not copyrightable)
- Card oracle text is a grey zone — tolerated by WotC for free fan projects
- Card images: do NOT bundle or use — clearest infringement vector
- Custom card rendering (no images) sidesteps the image issue entirely
- Must remain free — WotC fan content policy prohibits monetization
- Do not use official card frames or WotC branding
- Project name must not imply WotC affiliation
- Prior art: XMage and Forge have operated in this grey zone for 15+ years without legal action

---

## Component 2: React Web Client

### Purpose
A "dumb" renderer. It displays the GameView the server sends and translates player input into action_id selections. It contains zero game logic and zero rules knowledge.

### Core Principle
`valid_actions[]` drives all interactivity. Anything not in the list is visually dimmed and non-interactive. The client never computes whether an action is legal — it trusts the server completely.

### Rendering Architecture: Hybrid DOM + Canvas

**React DOM handles:**
- Lobby, matchmaking, room list
- Player's own hand (full-size cards, max 10, no performance concern)
- Action bar (valid_actions rendered as labeled buttons)
- Life totals, mana pools, graveyard/exile counts
- Card detail popover (hover or tap on any card shows oracle text)
- Stack visualization
- All UI chrome (settings, profile, deck builder)

**Pixi.js WebGL canvas handles:**
- The battlefield only
- All permanents for all players
- Handles zoom, pan, hover, tap-to-expand
- GPU-accelerated, handles 100+ objects without DOM overhead

**Integration:** React renders a single `<canvas>` element. The Pixi scene graph inside it is managed via `react-pixi-fiber`, allowing Pixi objects to be written as React components.

### Custom Card Rendering (No Images)

Cards are procedurally rendered from card data. No image downloads. Benefits:
- No bundle size impact from 30,000 card images
- Cards can be dynamically resized without quality loss
- Color identity is always accurate (not dependent on art)
- Sidesteps the clearest copyright infringement vector

**What gets rendered at battlefield scale:**
- Background color fill derived from color identity (W/U/B/R/G/Gold/Colorless)
- Card name (text)
- Mana cost (colored pip icons as SVG paths)
- Power/toughness if creature
- Tapped state (rotation)
- Any relevant counters (+1/+1, loyalty, etc.)

**What gets rendered on hover/tap (detail view):**
- Full oracle text
- Full type line
- Set symbol
- Everything visible on the physical card

**Color identity fills:**
- White: `#F9FAF4`, Blue: `#0E68AB`, Black: `#150B00`
- Red: `#D3202A`, Green: `#00733E`, Gold (multicolor): gradient
- Colorless: `#9C9B8E`

**Mana pip icons and card borders:** SVG paths drawn in Pixi, not rasterized images. Scale perfectly at any size, sharp on all DPI.

**Crisp rendering:** Canvas set at `window.devicePixelRatio` for retina/high-DPI sharpness.

### Dynamic Card Sizing

Card size is a function of permanent count in that zone:

- ≤8 permanents: full size (1.0×)
- ≤14 permanents: 0.75×
- ≤20 permanents: 0.55×
- 20+ permanents: 0.4× minimum readable size

At minimum size: colored rectangle + name label only. Oracle text always available on hover. Tokens and wide board states remain readable.

### Multi-Player Layout

**2 players:** Classic top-bottom split. Opponent top, local player bottom.

**3 players:** Triangle. Opponent zones top-left and top-right, local player bottom-center.

**4 players:** Four corners. Local player bottom-center, opponents top and sides.

**5–8 players:** Hub-and-spoke. Local player always at bottom with full zone. Opponents arranged radially using polar coordinates, each zone rotated to face center. As player count increases, opponent zones scale down and simplify to status strips (hand count, life total, permanent strip). Tap/click on any opponent zone opens a full detail drawer.

**Scaling per player count:**
- 2–4: Full hand visibility, detailed card renders for all players
- 5–6: Compact opponent hand (fan/stack), abbreviated zones
- 7–8: Icon-based opponent zones, tap-to-expand; mobile caps at 4 players for full UI

### Two UI Modes

**Overview mode** (default during opponents' turns):
- All players visible
- Permanents at reduced size
- Life totals and key counters visible
- Can pass priority from here
- No interaction detail

**Focus mode** (when it's your turn to make a meaningful decision):
- Your hand expands to full size
- Your battlefield comes to foreground
- Stack clearly visible
- Valid actions highlighted
- Opponents' zones shrink to status strips
- All interactive elements are determined by valid_actions[]

Transition between modes is contextual, driven by the priority_player field in GameView.

### Input Handling

Three input modes are supported. The interaction model is designed for touch first (most constrained), which naturally accommodates mouse and can be adapted to controller.

**Mouse:**
- Hover shows card preview without click
- Right-click for context menu
- Precision clicking on small targets acceptable

**Touch:**
- No hover state — single tap shows preview, second tap selects/acts
- Tap targets are larger (minimum 44px)
- Long-press substitutes for right-click
- Drag for attacking (drag creature to opponent/planeswalker)

**Controller:**
- D-pad moves focus through interactive elements
- Focus traversal graph covers all elements in valid_actions[]
- Only elements in valid_actions[] are focusable during a game action
- Bumpers switch zones, triggers confirm/cancel

### The Stack and Priority UI

The most critical UX challenge. The interface must make immediately obvious:
- What kind of decision is being requested
- Who has priority
- What happens if you pass

Action bar labels change based on context:
- "Pass priority" during spell resolution
- "Declare attackers" during combat
- "Choose target" during targeting
- These come from the `valid_actions[].label` field sent by the server

The entire UI chrome shifts visually when focus mode activates — the action bar becomes prominent, the stack expands, and invalid elements dim.

### Targeting Mode

When valid_actions includes a target-selection action:
- Targeting mode overlay activates
- Valid targets highlighted (derived from valid_actions[])
- Invalid targets dimmed and non-interactive
- Multi-target selection shows a counter
- Targets can be permanents on any player's battlefield, players (click player portrait), or cards in graveyards/exile (zone expands)
- Touch: highlighted targets have enlarged tap areas

### Combat UI

Declare attackers:
- Drag a creature to an opponent (or their planeswalker) to declare it as an attacker
- Tap/click alternative for touch/controller
- Attacking creatures shown with visual indicator and an arrow/line to their target

Declare blockers:
- Opponent's interface during attacker's attack step
- Drag their creatures onto attacking creatures to block
- Multiple blockers can be assigned to one attacker

Damage assignment:
- For multiply-blocked creatures, drag to set order
- For multiple targets of abilities, same drag mechanic

### Technology Stack

| Concern | Technology |
|---|---|
| UI framework | React 18+ |
| Battlefield rendering | Pixi.js (WebGL) via react-pixi-fiber |
| Component library | shadcn/ui + Radix UI |
| Styling | Tailwind CSS |
| State management | Zustand (client state only — server is source of truth) |
| WebSocket | Native WebSocket API with reconnect logic |
| Typography | Inter or similar single variable font |
| Build | Vite |

### Deployment

The React client is a static build (HTML + JS + CSS). It can be:
- Served from any static host (S3, Cloudflare Pages, nginx)
- Bundled inside a Tauri desktop app alongside the Rust server binary
- Converted to a PWA for web offline (Rust WASM runs in Web Worker)

For desktop: Tauri (Rust-based shell) wraps the React UI and manages the bundled Rust game server as a child process. Same React code, different wrapper.

---

## Shared: Card Data

**Source:** Scryfall API for online play. Bundled JSON snapshot for offline play.

**Fields needed per card for rendering without images:**
- `name`
- `mana_cost` (parsed into pip array)
- `color_identity` (array of W/U/B/R/G)
- `type_line`
- `oracle_text`
- `power` / `toughness` (nullable)
- `loyalty` (nullable, for planeswalkers)
- `keywords` (array)
- `layout` (normal, transform, adventure, etc.)

**Card database in the Rust engine:** Loaded at startup from JSON. Cards are immutable data. The engine references cards by `CardId` (a stable integer or UUID) and looks them up in a static map.

---

## Development Sequence

1. **Rust rules engine core** — GameState, phase FSM, action validator, action generator producing valid_actions[]
2. **WebSocket server wrapper** — rooms, lobby, session management
3. **CLI client** — proves the API works, tests rules without UI
4. **LLM agent via CLI** — feed GameView JSON to an LLM, parse response, send action — AI opponents working
5. **React client** — connects to WebSocket, renders GameView, sends actions
6. **Multi-player layouts** — 4-player, then 6-player, then 8-player
7. **Bundling** — Tauri desktop wrapper + bundled Rust binary
8. **Mobile** — after everything else is stable

---

## What This Project Is Not

- Not a deck builder (integrate with existing tools like Moxfield or Archidekt)
- Not a card collection manager
- Not affiliated with Wizards of the Coast
- Not monetized in any form
- Not using official MTG card images or card frame designs
- Not a 3D or particle-effects experience (clean 2D only)
