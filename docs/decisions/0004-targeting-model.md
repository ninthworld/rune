# ADR 0004: End-to-end targeting model

- Status: accepted
- Date: 2026-07-30

## Context

Targeting is the first mechanic that crosses every layer of the system at once. A removal
spell, a burn spell, or a counterspell needs a representation in the effect IR, an entry in
legal-action generation, storage on the stack, a re-check at resolution, a wire shape that
carries the choice, and a client interaction that assembles it. Getting these to agree
afterwards is far more expensive than deciding them together.

Four existing rules constrain the design:

- **Zero game logic in the client.** Which targets are legal, and how many, is a rules
  question the engine answers; the client renders choices the server already validated.
- **Zero I/O and value semantics in the engine** (ADR 0001, ADR 0003). A target must be plain
  `Clone`/`Eq` data with no closures, and legality must be a pure function of current state.
- **The protocol is a contract.** The two-message shape (`GameView` out, `ChooseAction` in)
  is deliberately minimal, and a targeted action must extend it without giving the client
  rules knowledge and without making the client stateful across messages.
- **Legal-action generation is the core complexity** (ADR 0001). `apply_action` validates a
  chosen action by regenerating the legal list and checking membership. Pre-enumerating every
  legal *combination* of targets would make that generator combinatorial — an N-candidate,
  k-target spell is O(Nᵏ) — so the enumeration strategy is load-bearing.

Targets must also name a *specific copy* of a card. Per-instance identity is a hard
prerequisite: with bare printed `CardId`s, two Forests are indistinguishable and "target
*that* Forest" is inexpressible.

## Decision

One targeting model runs the length of the stack: the engine owns target specs, legal-set
enumeration, and resolution-time re-checking; the protocol carries a content-bound action;
the client renders the choice as data and computes no legality.

### Engine

- **A target vocabulary in the effect IR** (`crates/sage-engine/src/ability.rs`), in two
  parts, both plain `Clone`/`Eq`/`Deserialize` data:
  - A **`TargetSpec`** declaring what an effect *may* target — a small closed enum of
    predicates (any creature, any permanent, any player, a spell on the stack, any target),
    authored as card data like every other IR node. Effects with an implicit subject, such as
    `DrawCard`, carry none.
  - A **`Target`** value: a resolved reference to a specific object — a `CardInstanceId`, a
    `PermanentId`, a `PlayerId`, or a `StackId`. Never a bare printed `CardId`.

  An effect declares its slots as an **ordered list of groups** (`Effect::target_groups`),
  each a `{spec, min, max}` (ADR 0017 §5). The list is what lets one effect name two
  *differently* specified slots — a fight's "target creature you control" and "target creature
  you don't control" — without splitting into two effects that would be aimed independently.
  An effect whose slots do not share a spec acts on all of them or on none (CR 701.12c);
  within a single group the CR 608.2c per-target re-check still applies.
- **Chosen targets are stored on the stack.** `StackObject` carries the targets recorded when
  the spell or ability was put on the stack (CR 601.2c — targets are chosen on announcement,
  not on resolution). The stack stays a complete, inspectable record: a view can show
  "Lightning Strike targeting that creature" with no side lookup.
- **Legality is re-checked on resolution, and objects fizzle.** On resolution the engine
  re-evaluates each stored target's spec against *current* state. Targets that are now
  illegal are skipped; an object all of whose targets are illegal is removed from the stack
  without resolving (CR 608.2b). This is a pure check in the resolution path — no listener
  watches targets, resolution simply re-derives legality.

### Enumeration

- **Actions are parameterized by their chosen targets, not pre-multiplied into one variant
  per combination.** `valid_actions` advertises a targeted action once, together with the
  legal candidate set *per target slot* — O(N) per slot, never the O(Nᵏ) cartesian product.
- **Validation regenerates legal sets, not the combination list.** `apply_action` keeps its
  regenerate-and-check discipline, but checks the chosen targets against freshly computed
  legal sets for that action rather than requiring the exact chosen `Action` value to appear
  in an exhaustive list. A chosen target outside its legal set makes the action a no-op,
  exactly as an illegal action is.

### Protocol

- **A targeted action carries its requirements.** A `ValidAction` may carry an ordered list of
  requirement steps, each a target slot with its spec label and the entity ids of its legal
  candidates. The client walks the list as a prompt queue and answers **atomically** — one
  `ChooseAction` submitting the full selection, never a stateful multi-message handshake.
- **Content binding via a token.** `ChooseAction` carries an opaque server-issued token bound
  to the exact action content (kind, subject, requirements) the client is answering. The
  server verifies the returned token against the action it currently offers under that id; a
  mismatch is rejected and the current view re-sent. The token is a hash of the action's
  content rather than a random nonce, so it is **stateless on the server** — the room
  recomputes it from the regenerated action and needs no per-id secrets, which keeps reconnect
  a plain re-send.

  This matters beyond targeting. Action ids are positional over a freshly regenerated list,
  which is safe only while decisions are strictly sequential and the priority holder is the
  sole mutator. The moment decisions stop being sequential — simultaneous choices, triggers a
  non-active player orders, a multi-step selection — a stale id could silently rebind to a
  different action. Content binding closes that hole structurally.

### Client

Targeting is data-driven and subject-owned. The client enters targeting mode purely from the
requirement steps in the view: it highlights exactly the candidate entity ids the server
listed and dims everything else, computing no legality of its own. Target picking is
select-then-confirm on the target entity — the same interaction as every other action, so a
target is just another entity the player selects. The assembled selection is submitted
atomically with its token, and the entire targeting UI is reconstructable from one `GameView`
plus the pending prompt.

### The mana sub-choice rides the same machinery

Choosing which mana pays a cost is structurally identical to choosing a target: the engine
offers legal options, the player answers atomically, the answer is content-bound. When manual
mana payment is implemented it reuses the requirement-step and token mechanism rather than
growing a second sub-choice protocol. That generality is a constraint on the protocol shape,
not an afterthought.

## Consequences

- **Easier.** Each layer has a decided contract to build against, so a targeted card is a
  bounded change rather than a simultaneous rewrite. The stack becomes a complete record with
  targets included, so views, replay, and AI search get targeting for free. Content binding
  eliminates a latent correctness bug before non-sequential decisions can expose it, without
  making the server stateful.
- **Harder / given up.** `Action`, `StackObject`, and the effect IR all grow a target
  dimension, and legal-action generation must compute candidate sets under an explicit
  O(N)-per-slot budget. `ChooseAction` stops being a single string: clients must send the
  token and the selection.
- **Not covered.** Hidden-zone targets are expressible in this model but are not built; each
  such addition is a change to the vocabulary, not to the model. "Up to N targets" (ADR 0017
  §5) and slots with per-slot specs (issue #737) have since been built exactly that way — as
  additions to the vocabulary that left the model, the protocol, and the client untouched,
  which is the load-bearing claim this ADR made.
