# MTG Client — UI Requirements

Everything the UI must be able to represent or make possible. Server is authoritative for rules
and legality (`GameView` + `valid_actions`); the client renders state and collects choices. Each
item is a capability the UI must support, not necessarily in v1 — see Scoping at the end.
Requirements marked **[stress]** interact dangerously with current design decisions and are
analyzed in the final section.

---

## 1. Zones

- Battlefield per player, with the controller/owner distinction (see 2. Control changes).
- Hand: own hand fully visible; opponents as counts.
- Persistently revealed hands (e.g. Telepathy-style effects) — an opponent's hand rendered open, indefinitely. **[stress]**
- One-time hand inspection ("look at target player's hand") — modal browse of a snapshot.
- Library: count always; revealed top card (Courser-style); playable-from-top states; known-to-you-only top card (after scry, if server exposes it).
- Graveyard: public, ordered, browsable per player; order matters for some effects.
- Exile: public; face-down exiled cards (unknown); exiled "with"/"by" a specific permanent (imprint, adventure, foretell, impulse-draw "may play until end of turn") — exiled cards must be visually associated with their source or their playability window. **[stress]**
- Stack as a first-class zone (see 4).
- Command zone: commanders (with cast-count/tax), emblems, ongoing schemes/planes if ever supported.
- Sideboard: visible between games; wish-style access mid-game if the format allows.
- Zone browsers: any public ordered zone openable as a full overlay with scrolling; filter/search once counts exceed ~20 (Commander graveyards reach 40+).
- Token existence rules: tokens cease to exist outside the battlefield — animations must not imply they went to the graveyard zone browser (they appear in log only).
- Cards "under" other permanents (mutate stacks; anything the server models as a merged object) — one battlefield object composed of multiple cards, inspectable as a list. **[stress]**

## 2. Battlefield object state

Every permanent render must be able to carry, simultaneously:

- Tapped / untapped.
- Summoning sickness (creatures only, only when it constrains action).
- Counters: +1/+1 and −1/−1 (net displayed), loyalty (planeswalkers — the P/T slot becomes loyalty), and **arbitrary named counters** (charge, quest, fade, oil, "any keyword" counters, custom). Generic rendering: up to ~3 badge slots at battlefield size, full list in inspect. **[stress]**
- Damage marked this turn (distinct from toughness reduction).
- Computed P/T: always display effective values; printed values only in inspect. Indicate direction of modification (buffed/shrunk).
- Attachments: auras, equipment, fortifications — visual link between attachment and host; attachments are permanents themselves (targetable, with their own state). Multiple attachments per host. Attachment chains (aura on an equipment). **[stress]**
- Face-down permanents (morph/manifest/cloak): render as anonymous 2/2 to opponents; owner sees the real card via inspect; owner-only "you know this" affordance. Face-down is also a *stack* state (casting face-down).
- Double-faced / transformed cards: current face renders; other face one tap away in inspect. Meld (two cards become one oversized object).
- Copies and clones: a permanent whose printed card differs from its current characteristics — render current characteristics, inspect shows both.
- Control vs ownership: a permanent controlled by me but owned by an opponent renders in **my** battlefield (control determines zone placement), with an ownership marker. On game loss/leave in multiplayer, owned cards leave all zones. **[stress]**
- Phased out: invisible-but-present; render as ghosted or hidden with a count ("2 phased out"), must not be targetable.
- Temporary status flags with gameplay weight: goaded, monstrous, renown, "doesn't untap next untap step", "attacks each combat if able", can't-block, exerted. Generic mechanism: server sends status strings → badge glyph if mapped, generic dot + inspect text if not.
- Granted/lost abilities (temporary flying, lost deathtouch): shown in inspect; optionally keyword micro-icons at battlefield size for combat-relevant ones (flying, first strike, deathtouch, trample, lifelink, vigilance, menace, reach, indestructible, hexproof).
- Sagas and Classes: chapter/level counter plus "which abilities are live" in inspect.
- Vehicles: crewed/uncrewed (is-a-creature-right-now state).
- Battles (siege): a permanent with defense counters and a designated protector, attackable like a player. **[stress]** (extends "attack target" set)
- Collapse/stack grouping key must include *all* of the above: only truly identical objects group. ×N badge shows count; partial selection out of a collapsed stack must be possible ("sacrifice two of your nine Saprolings" → prompt expands the stack temporarily).

## 3. Players and global state

- Life totals (large deltas: Commander starts at 40, combos reach 4-digit life — layout must fit 4 digits).
- Poison counters, energy, experience, rad counters — per-player resource counters, zero-suppressed.
- Commander damage: an N×M matrix (damage from each commander to each player); surfaced per-player in expanded tile / inspect, with a threat warning near 21. **[stress]**
- Player designations: monarch, initiative (+ current dungeon room), city's blessing, day/night (global), ring-bearer/tempted count. Global designations get one shared slot in the phase ribbon; per-player ones live on the player tile.
- Floating mana: hidden at zero; when mana is in a player's pool, a pip strip appears near their avatar and persists until it empties. **[stress]** (violates "nothing appears/disappears" if not reserved)
- Turn order visualization for 3–8 players (who is next; turn direction if it can change).
- Team constructs: Two-Headed Giant (shared life, shared turn, two hands on "my" side) — see Scoping. **[stress]**
- Dead/eliminated players: tile persists, dimmed, with elimination cause; their stuff leaves play.

## 4. Stack, priority, and timing

- Stack items in resolution order; each shows: source card render, controller, chosen modes/X/values, and **targets** (on-demand arrows or inspect list). Targets may themselves be stack items (counterspells) — arrows between stack entries.
- Triggered abilities: stack objects with **no card** — synthetic render: source card's frame identity + name, trigger text as the body. Same for activated abilities on the stack. **[stress]**
- Copies of spells on the stack (may have different targets each).
- "Can't be responded to" (split second) — visual lock on the stack + suppressed priority affordances.
- Priority indicator: who holds it, at what step, always visible (established: avatar ring + ribbon).
- Priority automation (the single biggest UX lever in a priority-accurate client):
  - Auto-pass when holding no instants/abilities (server can hint "nothing castable").
  - Stops system: pass until end of turn / until my next turn / to next phase, with standing per-phase stops the player can toggle (MTGO F-keys, Arena full-control equivalents). Cancelable mid-skip when something is put on the stack.
  - Full-control toggle (never auto-pass).
  - Auto-yield to a specific repeated trigger ("resolve all 30 of these").
  - Hold priority after casting (cast + retain, for storm-style sequencing).
- Trigger ordering: when multiple of my triggers fire simultaneously, an order prompt (this is a prompt-queue `order` type).
- Mid-resolution choices: resolution of one stack object can spawn prompts (server-initiated prompt queues) — already covered by the prompt architecture, but the UI must render "we are inside the resolution of X" context in the banner.

## 5. Turn structure

- Full phase/step model (untap, upkeep, draw, main 1, begin combat, declare attackers, declare blockers, first-strike damage, damage, end combat, main 2, end, cleanup) — displayed compactly, expandable; current step always visible.
- Extra turns / extra combats / added phases (the ribbon is data-driven, not a fixed sequence).
- Turn/decision timers: per-decision countdown, chess-clock total, timeout warnings; timer visibility for all players.
- Simultaneous multi-player decisions (e.g. "each player discards"): my prompt shows while others' pending status shows on their tiles ("choosing…"). **[stress]** (prompt system must not assume one active decider)

## 6. Prompt types (superset of the prompt queue)

Established: uniform queue of prompts per action, atomic submit, server-supplied legal sets. The
full set of prompt types the queue must support:

- `target` — pick 1 from legal set (established).
- `targets_n` — pick exactly N / up to N / any number; running count in banner; same-target-twice rules server-enforced.
- `divide` — distribute a fixed quantity (damage, counters) across chosen targets: +/− steppers per target, remaining pool in banner. No drag-dependence. **[stress]** (first prompt type needing per-target numeric input)
- `mode` — choose one/two/N modes; entwine/spree variants (modes have costs).
- `x_value` — numeric input with server-supplied max (or unbounded); slider + stepper + direct entry.
- `option` — yes/no or pick-from-list ("you may draw a card", choose a color, choose odd/even, choose a number).
- `name_a_card` — free-text typeahead over the full card database; needs on-screen keyboard path for controller/touch. **[stress]**
- `order` — arrange a small set (scry/surveil piles, trigger order, blocker damage order, library bottom order): list with move-up/down + drag as enhancement.
- `split_piles` — divide revealed cards into piles (Fact-or-Fiction style).
- `search` — full library/zone browse with filters ("search for a Forest card"): overlay browser, may-fail-to-find needs an explicit "find nothing" button.
- `select_from_zone` — choose K cards from hand/graveyard/etc. as a cost or effect (discard two, sacrifice a creature, exile from graveyard).
- `cost_option` — alternative and additional costs presented as distinct action variants *before* the queue (kicker, flashback, foretell, overload, casting face-down): `valid_actions` should enumerate these as separate actions or as a leading `mode` prompt — pick one convention and keep it.
- `mana_payment` — normally automatic; becomes a prompt only when the choice is meaningful (which color, which restricted source, pay life for shocklands/fetches at cast time). Confirm state previews the auto-tap with override.
- `assign_attack_target` — per attacker in multiplayer: which player/planeswalker/battle (see 7).
- Prompts during opponents' turns and server-initiated prompts (discard, blockers, "target player sacrifices") use the same renderer; cancel disabled, initiator named.
- Every prompt must expose: prompt text, progress (k of n), what-is-this inspect on the source, legal-set spotlight, and (where allowed) cancel/back.
- Fizzle/illegality feedback: if a submitted action is rejected (race with another player's response), a non-blaming error toast + state resync; if a resolved spell's targets became illegal, the log explains it.

## 7. Combat

- Declare attackers: multi-select creatures; select-all-legal shortcut; per-attacker attack target in multiplayer (player, planeswalker, battle) — default target + tap-to-retarget. **[stress]** (many-to-many across opponent tiles)
- Attack constraints surfaced: must-attack (pre-selected, locked), can't-attack (not selectable), goad (must attack, not the goader), attack costs ("pay {1} to attack") folded into confirm.
- Declare blockers: assign each of my creatures to an incoming attacker; multiple blockers on one attacker; menace minimums server-enforced but UI shows the requirement.
- Damage assignment: ordering blockers (attacker's choice) and manual damage splits (deathtouch/trample) via the `divide` prompt.
- Combat lines: attacker→defender and blocker→attacker links readable at 10+ creatures a side; hover/focus isolates one creature's links; full spaghetti never drawn at once. **[stress]**
- First-strike combat: two damage steps visibly distinct in the ribbon.
- Mid-combat state changes: creatures removed from combat, blockers destroyed pre-damage, flash blockers, ninjutsu swap — combat visualization must re-render from state, not from an animation script.
- Combat preview: focused pairing shows predicted damage/deaths (client-side estimate, display only).

## 8. Match, session, and multiplayer lifecycle

- Formats: 1v1 (Bo1/Bo3), FFA 3–8, Commander (per-format chrome: commander zone, tax, cmd damage), Brawl. 2HG/Archenemy/Planechase: see Scoping.
- Sideboarding screen between games; opening-hand mulligan flow (London: keep/mull, then bottom N via `select_from_zone` + `order`).
- Opening-hand actions (Leylines/Chancellors): a pre-game prompt window.
- Concede (always available, confirm-gated), draw offers/votes, takeback requests if the server permits (casual) — UI treats a granted takeback as a state resync, nothing special.
- Player elimination mid-game (FFA): tile → dead state; all their objects leave; triggers from that render in log.
- Disconnect/reconnect: **the entire UI must be reconstructable from a single GameView + current pending prompt.** No client-side state is load-bearing beyond ephemeral selection. This is the architectural keystone; every feature above must respect it. **[stress-test for everything]**
- Spectator mode: overview-mode-only rendering fed by a redacted GameView (no hidden info) — falls out free if the client never assumes it owns a hand.
- Rated/timed vs casual affordances (timers on/off, undo on/off) are server flags the client reads.

## 9. Information, history, and comprehension

- Game log: every state change as a human-readable line; entity names clickable → inspect; filterable by player; collapsible spam (30 identical triggers → one line ×30).
- Catch-up affordance: after being away/tabbed out, "what changed" — at minimum, change-pulse highlights on tiles and unread-log marker.
- Inspect (universal, established): full oracle text, current characteristics vs printed, all counters/statuses, attachments list, linked cards (DFC back, meld partner, tokens it creates, the card an adventure came from), zone-specific info (exiled-by, cast-count).
- Related-card previews: token-producing cards can show the token's render in inspect before it exists.
- Revealed-information ledger: things I've legitimately learned (opponent revealed a card, I saw a hand) — at minimum the log records it; optionally a pinned "known cards" note. Never re-derive hidden info client-side; only render what GameView says I may see.
- Counts everywhere: hand/library/graveyard/exile sizes visible per player at all times (established in tile digest).
- Storm count / cast-this-turn, when a relevant card is in play (contextual, zero-suppressed).

## 10. Input, accessibility, and settings

- Full parity across mouse, touch, controller via select/confirm/inspect/back/cancel event layer (established). No action reachable only by drag, hover, or keyboard.
- Keyboard as a fourth input: space/enter = confirm-primary, esc = cancel, F-key-style pass shortcuts, tab-cycle prompts.
- Colorblind safety: identity never encoded by hue alone (monogram letters, pip letters — established); targeting/selection rings differ in weight and shape, not just color; optional patterns.
- Text scale setting (all sizes derive from the three card tiers + one UI scale factor); minimum touch targets maintained at every scale.
- Reduced motion: every animation has a cut-to-end form; resync/replays skip animations entirely.
- Audio + attention cues: sound and OS notification on receiving priority/a prompt while unfocused (async multiplayer priority is unplayable without this); per-cue mute.
- Screen-reader pass over prompts and log at minimum (prompt text, legal option list, and log lines are all already text — keep them text, not baked into canvas). **[stress]** (argues for prompts/log/HUD in DOM, only battlefield in Pixi)
- Left/right-handed layout mirror on touch.
- Settings for every automation: stops, auto-pass, auto-yield, auto-tap, skip-confirm, auto-answer-single-option — per established defaults.

## 11. Rendering and performance envelope

- Board scale targets: 100+ permanents on one battlefield (token decks), 8 battlefields live, 60+ card zone browsers, log of thousands of lines — collapsing, virtualization, and texture caching sized for this.
- Animation system: a queue decoupled from state; state is always already-final (render from GameView, animate the diff); input on a live prompt is never blocked by pending animations; simultaneous events (board wipe) animate as one batch, not 40 sequential deaths.
- Deterministic layout: identical GameView → identical layout (reconnect, spectators, and bug reports depend on it).
- All text in the Pixi layer via cached bitmap text; anything needing selection/screen-readers/input lives in DOM above the canvas.

---

## Stress analysis — requirements vs. locked design decisions

**Holds as designed:**
- Prompt queue: every prompt type above fits the queue shape (some add new `type`s and input widgets, none breaks banner/spotlight/progress/atomic-submit).
- Reconstructable-from-GameView: already implied by server-authoritative valid_actions; elevate it to a hard invariant now and features 1–9 stay cheap.
- Collapsing rule: survives because grouping was defined on full state identity from the start; add "temporary expansion when a prompt's legal set intersects a collapsed stack."
- Tile model: eliminations, designations, commander damage, and simultaneous-decision status all live naturally in tiles (compact digest ⇄ expanded).

**Needs extension, not redesign:**
- Card render: add a generic badge system (max ~3 at battlefield size, overflow "+" into inspect) and keyword micro-icons. The monogram/frame/pill structure is untouched.
- Stack: add the synthetic ability-card render (source frame + trigger text). Decide its visual early — it is on screen constantly.
- Battlefield rows: attachments require host+attachment clusters as layout units (a cluster is one flex item); this changes the row layout algorithm, not the design.
- Phase ribbon: make it data-driven (list of steps from server) rather than hardcoded; reserve a slot for global designations and floating mana (zero-suppressed but space-stable).
- Action variants: pick the convention for alternative costs (separate valid_actions entries recommended — keeps the queue linear) before implementing the action bar grouping ("Cast Thicket Warden ▾" when >1 variant of the same card).

**Genuine threats — decide before building:**
1. **Control-change + ownership** (2): if the Pixi scene assumes a card sprite lives in its owner's container forever, Control Magic breaks it. Make zone placement follow *controller* in the layout function from day one, with owner as a render property.
2. **Multiplayer combat targeting** (7): attack arrows from my battlefield to compact tiles need tiles to be first-class targets and need an "isolate one creature's lines" interaction. Prototype this at 4 players before finalizing tile sizes.
3. **DOM/canvas split** (10): retrofitting accessibility onto an all-canvas UI is a rewrite. Decide now: battlefield/hand/stack in Pixi; prompts, action bar, log, tiles' text, and banners in DOM. This also makes the prompt system trivially screen-readable.
4. **Text-input prompts on controller** (6, name_a_card): needs an on-screen keyboard + typeahead; low frequency, high cost — fine to stub, but reserve the prompt `type` and the input-adapter event for it.
5. **2HG / teams** (3): breaks "me alone at the bottom." Recommend declaring it out of scope for the layout v1 and re-deriving the bottom band as a "seat" abstraction (a seat = 1+ players) only if/when needed — do not contort the current layout for it now.

## Scoping suggestion

- **v1:** 1v1 + FFA to 4; prompt types target/targets_n/mode/x/option/order/select_from_zone/search; combat with manual everything; log; inspect; reconnect; mouse+touch.
- **v1.5:** Commander chrome (tax, cmd damage matrix), 5–8 players, divide/split_piles/name_a_card, controller input, stops/automation suite, spectator.
- **Explicit non-goals until demanded:** 2HG/teams, Archenemy/Planechase, ante, banding, sub-game mechanics (Shahrazad), Un-set mechanics (stickers/attractions).
