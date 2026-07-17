# ADR 0009: Targeting and atomic action choices

- Status: accepted
- Date: 2026-07-11
- Issue: #55

## Context

Targets must identify physical game objects, remain legal under changing state, avoid
combinatorial action enumeration, and cross the protocol without teaching the client rules.
Positional action ids also need protection from stale views.

## Decision

### Engine model

Target specifications are closed data predicates in the effect IR. Chosen targets use
per-game `CardInstanceId`, `PermanentId`, `PlayerId`, or stack-object identity rather than
catalog `CardId`.

Targets are chosen when a spell or ability is announced and stored on its stack object. The
engine checks them again on resolution. Illegal targets are skipped; an object whose targets
are all illegal does not resolve.

`valid_actions` advertises one parameterized action with a legal candidate set for each slot.
It does not enumerate every combination. Applying an action regenerates the current legal
sets and validates the submitted choices against them.

### Protocol model

A `ValidAction` carries:

- ordered target `requirements`, each with an opaque slot and legal candidates;
- tagged non-target `prompts` using the same slot model; and
- a content-binding `token` derived from the action’s exact content.

The client gathers every slot answer and returns one atomic `ChooseAction`. The server
regenerates the action, verifies the token, and revalidates all choices. A stale action id
therefore cannot silently refer to changed content, and the server stores no per-view nonce.

The client highlights only server-supplied candidates and computes no target legality. Prompt
selection is ephemeral and reconstructable from the latest view.

### Enumeration constraint

Candidate enumeration is linear in the available objects per slot. Interactions between slots,
cardinality, optional selection, distinctness, and action-specific rules are checked by the
engine during validation rather than encoded as a client rule.

## Consequences

Targets, combat declarations, mulligan choices, discards, and ordering can share one atomic
choice mechanism. Stack state is self-contained and stale actions fail safely. The engine and
protocol carry more structured data, and every new prompt shape must be implemented across the
engine, protocol, server projection, and clients.
