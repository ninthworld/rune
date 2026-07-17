# ADR 0009: End-to-end targeting model (engine → protocol → client)

- Status: accepted
- Date: 2026-07-11
- Issue: #55

## Context

Targeting is absent from every layer of RUNE at once. No `Effect`, `Action`, or
`StackObject` carries a target (`ability.rs:60-74`, `lib.rs:38-54`, `stack.rs`);
`valid_actions` never advertises one; the wire protocol has only a placeholder
`requirements` note (`docs/protocol.md:44-47`); and the client has no way to pick
one. The first removal, burn, or counterspell is therefore unrepresentable, and
adding it touches `Action`, the `valid_actions` generator, the stack, resolution,
the two-message protocol, and the client simultaneously. That simultaneity is why
the 2026-07-11 sustainability review flagged targeting as the single biggest
retrofit cliff, and why it is worth deciding on paper before the first targeted
card ships. This ADR is that decision; the implementation is split into the
follow-up issues listed at the end.

The forces that constrain the design are the project's existing hard rules:

- **Zero game logic in the client** (`AGENTS.md`). Which targets are legal, and
  how many, is a rules question the engine must answer; the client may only render
  choices the server already validated.
- **Zero I/O and value semantics in the engine** (ADR 0002, ADR 0007). Any target
  representation must be plain `Clone`/`Eq` data with no closures or listeners, and
  legality must be a pure function of current state, recomputed on demand.
- **The protocol is a contract** (`AGENTS.md`, ADR 0002). The two-message shape
  (`GameView` out, `ChooseAction` in) is deliberately minimal; a targeted action
  must extend it without giving the client rules knowledge and without making the
  client stateful across messages (`docs/protocol.md:59-63`).
- **`valid_actions` is the core complexity** (ADR 0002). `apply_action` validates a
  chosen action by regenerating the full legal list and checking membership
  (`lib.rs:145-147`). Naively pre-enumerating every legal *combination* of targets
  turns that generator combinatorial (an N-target-set, k-target spell is O(Nᵏ)),
  so the enumeration strategy is load-bearing, not incidental.

Two adjacent problems share machinery with targeting and are called out
explicitly:

1. **Content binding.** Action ids are positional `a{index}` strings over a
   freshly regenerated list, matched by id + priority holder only
   (`view.rs:178-184`, `view.rs:321-334`). This is safe *only* because the priority
   holder is the sole mutator and decisions are strictly sequential: the list a
   client answers is the list the server regenerates. The moment decisions stop
   being strictly sequential — simultaneous choices, triggers a non-active player
   must order, a multi-step target selection whose intermediate state the server
   does not persist — a stale `a2` can silently rebind to a *different* action. A
   targeted action is the first multi-step decision, so it is where this must be
   fixed.
2. **Sub-choices in general.** Auto-payment currently spends generic mana in a
   fixed color order (`mana.rs:105-134`), papering over a real player choice ("which
   mana pays this?"). That is the same shape as "which target?" — a sub-choice the
   engine must offer and the player must answer — so the protocol machinery this
   ADR introduces should be able to carry it later.

Targets must also name a *specific copy* of a card. Zones store bare `CardId`s
today and the view acts on "the first matching copy" (`view.rs:28-34`), so two
Forests are indistinguishable — you cannot say *which* one a spell targets. Per-
instance identity (#51) is therefore a hard prerequisite and is treated as one.

## Decision

RUNE gets a single targeting model that runs the length of the stack: the engine
owns target specs, legal-set enumeration, and resolution-time re-checking; the
protocol carries a content-bound multi-step action; the client renders the choice
as data and never computes legality.

### Engine

- **A `Target` vocabulary lives in the effect IR** (`crates/rune-engine/src/ability.rs`),
  in two parts, both plain `Clone`/`Eq`/`Deserialize` data (no closures — ADR 0007):
  - A **target spec** declaring what an effect *may* target: a small closed enum of
    predicates (e.g. any creature, any permanent, any player, an instance in a
    named zone), authored as card data like every other IR node. An `Effect` that
    targets carries its spec(s) explicitly; effects with an implicit subject (e.g.
    `DrawCard`, whose comment already says it "needs no target") carry none.
  - A **chosen target** value: a resolved reference to a *specific* game object — a
    card `CardInstanceId` (#51), a `PermanentId`, or a `PlayerId`. Never a bare
    printed `CardId`; that is the whole reason #51 blocks this.
- **Chosen targets are stored on the stack.** `StackObject` (`stack.rs`) gains a
  targets field carrying the chosen targets recorded when the spell/ability was put
  on the stack (CR 601.2c — targets are chosen on announcement, not on resolution).
  This keeps the stack a complete, inspectable record: a `GameView` can show
  "Lightning Bolt targeting that creature" with no side lookup.
- **Legality is re-checked on resolution, and objects fizzle.** When an object
  resolves, the engine re-evaluates each stored target's spec against *current*
  state. Targets that are now illegal are skipped; an object all of whose targets
  are illegal is removed from the stack **without resolving** (CR 608.2b — the
  spell/ability "doesn't resolve"). This is a pure check in the resolution path
  (`resolve_stack_object`/`apply_effect`), consistent with the pull-based rule: no
  listener watches targets, resolution simply re-derives legality.

### Enumeration

- **Actions are parameterized by their chosen targets, not pre-multiplied into one
  variant per combination.** A targeted `Action` carries the selection the player
  made. `valid_actions` advertises the targeted action *once*, together with the
  **legal target set per target slot** — an O(N) list of candidates per slot, never
  the O(Nᵏ) cartesian product of combinations. This directly answers ADR 0002's
  "core complexity" warning: the generator stays linear in board size per slot.
- **Validation regenerates legal *sets*, not the full combination list.**
  `apply_action` keeps its regenerate-and-check discipline (`lib.rs:145-147`) but
  checks the chosen targets against freshly computed legal sets for that action,
  rather than requiring the exact chosen `Action` value to appear in an exhaustively
  enumerated list. A chosen target outside its legal set makes the action a no-op,
  exactly as an illegal action is today.

### Protocol

- **A targeted action is a multi-step action carrying its requirements.** We adopt
  the `requirements` extension already sketched at `docs/protocol.md:44-47` rather
  than inventing a parallel parameterized message: a `ValidAction` may carry an
  ordered list of **requirement steps** (each a target slot with its spec label and
  the entity ids of its legal candidates). The client walks the list as a prompt
  queue and answers **atomically** — one `ChooseAction` submitting the full
  selection, never a stateful multi-message handshake (preserves
  `docs/protocol.md:59-63`). `ChooseAction` gains a field for the chosen target
  entity ids keyed by slot.
- **Content binding via a token.** `ChooseAction` additionally carries a
  **content-binding token**: an opaque server-issued value bound to the exact action
  (kind + subject + target requirements) the client is answering. The server
  verifies the returned token against the action it currently offers under that id;
  a token that does not match is rejected and the current `GameView` is re-sent. We
  specify this as a **hash/echo of the action's content** (not merely a random
  nonce) so it is stateless on the server — the room need not remember per-id
  secrets, it recomputes the token from the regenerated action, keeping the
  full-state invariant that lets reconnect be a plain re-send. This closes the
  positional-`a{index}` rebinding hole (`view.rs:178-184`, `view.rs:321-334`) before
  the first non-sequential decision can exploit it.
- Protocol edits are a contract change (`AGENTS.md`): the concrete field shapes land
  with `rune-protocol` and `docs/protocol.md` together, in the protocol follow-up
  issue. This ADR fixes the model; it ships **no protocol type or wire change
  itself**, and deliberately leaves `docs/protocol.md` unmodified beyond its
  existing `requirements` note so nothing here can read as an implemented contract.

### Client

- **Targeting is data-driven and subject-owned.** The client enters targeting mode
  purely from the requirement steps in the prompt/`GameView` (`docs/brief.md`
  "Targeting Mode"): it highlights exactly the candidate entity ids the server
  listed and dims everything else, computing no legality of its own. Target picking
  is select-then-confirm on the target entity, consistent with ADR 0004's
  subject-owned routing — the same interaction model as every other action, so a
  target is just another entity the player selects. The assembled selection is
  submitted atomically with its content-binding token. The entire targeting UI is
  reconstructable from one `GameView` + pending prompt, so no client state is
  load-bearing across messages.

### Dependencies and the mana sub-choice

- **Per-instance identity (#51) is a hard prerequisite.** Chosen targets reference a
  specific `CardInstanceId`/`PermanentId`/`PlayerId`; the "first matching copy"
  workaround (`view.rs:28-34`) cannot express "target *that* Forest". Targeting
  implementation is blocked on #51 landing first.
- **Mana payment is the same shape, deferred to the same machinery.** Choosing which
  mana pays a cost (`mana.rs:105-134` currently auto-spends in a fixed order) is a
  player sub-choice structurally identical to choosing a target: the engine offers
  legal options, the player answers atomically, the answer is content-bound. When
  manual mana payment is implemented it should reuse the requirement-step + token
  mechanism this ADR defines rather than growing a second sub-choice protocol. It is
  **explicitly deferred**: no separate issue is filed here because it is downstream
  of the protocol shape (issue below) — that shape must generalize past targets to
  cover it, and that generality is a stated constraint on the protocol issue.

## Consequences

- **Easier:** the first targeted card becomes implementable as a bounded set of
  changes rather than a simultaneous rewrite; each layer has a decided contract to
  build against. The stack becomes a complete record (targets included), so views,
  replay, and AI search see targeting for free. The content-binding token
  eliminates a latent correctness bug (stale-id rebinding) before non-sequential
  decisions — triggers, APNAP ordering — can trigger it, and it does so without
  making the server stateful. Manual mana payment inherits the machinery instead of
  duplicating it.
- **Harder / given up:** `Action`, `StackObject`, and the `Effect` IR grow a target
  dimension, and `valid_actions` must compute legal target sets — the exact "core
  complexity" ADR 0002 named, now with an explicit O(N)-per-slot budget the
  enumeration must honor. `ChooseAction` stops being a single string; clients must
  send the token and the selection. Per-instance identity (#51) becomes a blocking
  dependency for any targeted card.
- **Deferred:** multi-target spells with per-slot distinctness/legality
  interactions, "up to N targets", targeting the stack (counterspells resolving
  against a stack object) and hidden-zone targets are all expressible in this model
  but are out of scope for the first slice; manual mana payment rides the same
  machinery later (above).

## Follow-up implementation issues

Each is one-PR sized and cites this ADR:

- **#70 — engine:** `Target` representation on the effect IR and stack; resolution-
  time legality re-check and fizzle.
- **#71 — engine:** enumerate legal targets in `valid_actions` via a parameterized
  action, without combinatorial blowup.
- **#72 — protocol:** multi-step targeted-action shape (`requirements`) + content-
  binding token; `rune-protocol` + `docs/protocol.md`.
- **#73 — server:** emit target requirements + token in the view (`view.rs`) and
  resolve the returned choice; depends on #51.
- **#74 — client:** targeting-mode UX driven entirely by `GameView`/prompt data.

Prerequisite: **#51** (per-instance card identity) must land before #70/#73.
